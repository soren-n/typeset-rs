//! Pass 8: DenullDoc → DenullDoc (reassociate after grp/seq removals)
//!
//! Right-associates composition trees (e.g. `Comp(Comp(a, b, p1), c, p2)`
//! becomes `Comp(a, Comp(b, c, p2), p1)`), treating Term/Fix/Grp/Seq as atoms
//! and reassociating inside each Grp/Seq independently. The original expressed
//! this with two nested continuations (a `partial` left-wrapper and a `cont`),
//! recursing on the native stack and aborting on deep inputs.
//!
//! Here the object walk is a descend/ascend trampoline: `partial` is a small
//! enum applied at each leaf, and `cont` is a heap-allocated frame stack. The
//! doc spine is a plain loop.

use crate::compiler::types::{DenullDoc, DenullObj};
use bumpalo::Bump;

/// A pending left-wrapper applied to a reassociated object at a leaf.
#[derive(Copy, Clone)]
enum Partial<'b> {
    /// Identity: pass the object through unchanged.
    Id,
    /// Wrap the object as the left operand of a composition with `right`.
    Comp { right: &'b DenullObj<'b>, pad: bool },
}

/// A pending continuation frame (the defunctionalized `cont`).
enum Frame<'b, 'a> {
    /// Wrap the ascending value in a Grp, then apply the saved partial.
    Grp(Partial<'b>),
    /// Wrap the ascending value in a Seq, then apply the saved partial.
    Seq(Partial<'b>),
    /// The left operand of a composition, still to be reassociated with the
    /// ascending value (the reassociated right operand) as its `right`.
    Comp { left: &'a DenullObj<'a>, pad: bool },
}

/// A doc-spine element with a tail (terminals are handled separately).
enum DocItem<'b> {
    Empty,
    Break(&'b DenullObj<'b>),
}

/// Reassociate after grp and seq removals
pub fn reassociate<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
    // Walk the linear spine to its terminal (Eod or Line), reassociating each
    // object, then fold the collected items back on.
    let mut items: Vec<DocItem<'b>> = Vec::new();
    let mut cur = doc;
    let terminal: &'b DenullDoc<'b> = loop {
        match cur {
            DenullDoc::Eod => break mem.alloc(DenullDoc::Eod),
            DenullDoc::Line(obj) => break mem.alloc(DenullDoc::Line(_reassoc_obj(mem, obj))),
            DenullDoc::Empty(doc1) => {
                items.push(DocItem::Empty);
                cur = doc1;
            }
            DenullDoc::Break(obj, doc1) => {
                items.push(DocItem::Break(_reassoc_obj(mem, obj)));
                cur = doc1;
            }
        }
    };
    let mut result = terminal;
    for item in items.iter().rev() {
        result = match item {
            DocItem::Empty => mem.alloc(DenullDoc::Empty(result)),
            DocItem::Break(obj) => mem.alloc(DenullDoc::Break(obj, result)),
        };
    }
    result
}

/// Reassociates a single object, right-associating its compositions.
fn _reassoc_obj<'b, 'a: 'b>(mem: &'b Bump, obj: &'a DenullObj<'a>) -> &'b DenullObj<'b> {
    let mut stack: Vec<Frame<'b, 'a>> = Vec::new();
    let mut cur = obj;
    let mut partial = Partial::Id;
    'machine: loop {
        // Descend, visiting a composition's right operand first, until a leaf
        // produces an ascending value.
        let mut value: &'b DenullObj<'b> = loop {
            match cur {
                DenullObj::Term(term) => {
                    break _apply_partial(mem, partial, mem.alloc(DenullObj::Term(term)));
                }
                DenullObj::Fix(fix) => {
                    break _apply_partial(mem, partial, mem.alloc(DenullObj::Fix(fix)));
                }
                DenullObj::Grp(obj1) => {
                    // Grp is a boundary: reassociate its contents afresh.
                    stack.push(Frame::Grp(partial));
                    partial = Partial::Id;
                    cur = obj1;
                }
                DenullObj::Seq(obj1) => {
                    stack.push(Frame::Seq(partial));
                    partial = Partial::Id;
                    cur = obj1;
                }
                DenullObj::Comp(left, right, pad) => {
                    stack.push(Frame::Comp { left, pad: *pad });
                    cur = right;
                }
            }
        };
        // Ascend, applying frames until we must descend a Comp's left operand.
        loop {
            match stack.pop() {
                None => return value,
                Some(Frame::Grp(p)) => {
                    value = _apply_partial(mem, p, mem.alloc(DenullObj::Grp(value)));
                }
                Some(Frame::Seq(p)) => {
                    value = _apply_partial(mem, p, mem.alloc(DenullObj::Seq(value)));
                }
                Some(Frame::Comp { left, pad }) => {
                    cur = left;
                    partial = Partial::Comp { right: value, pad };
                    continue 'machine;
                }
            }
        }
    }
}

/// Applies a pending left-wrapper to a reassociated object.
fn _apply_partial<'b>(
    mem: &'b Bump,
    partial: Partial<'b>,
    obj: &'b DenullObj<'b>,
) -> &'b DenullObj<'b> {
    match partial {
        Partial::Id => obj,
        Partial::Comp { right, pad } => mem.alloc(DenullObj::Comp(obj, right, pad)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::DenullTerm;

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    fn term<'b>(mem: &'b Bump, s: &'b str) -> &'b DenullObj<'b> {
        mem.alloc(DenullObj::Term(mem.alloc(DenullTerm::Text(s))))
    }

    #[test]
    fn reassociate_right_associates_deep_left_nested_comp() {
        let mem = Bump::new();
        // Left-nested: Comp(Comp(... Comp(a, b) ..., b), b).
        let mut obj: &DenullObj = term(&mem, "a");
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Comp(obj, term(&mem, "b"), false));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = reassociate(&mem, doc);
        let DenullDoc::Line(result) = out else {
            panic!("expected a line")
        };
        // The result is right-nested: a right spine of DEEP compositions.
        let mut count = 0usize;
        let mut cur: &DenullObj = result;
        while let DenullObj::Comp(_left, right, _pad) = cur {
            count += 1;
            cur = right;
        }
        assert!(matches!(cur, DenullObj::Term(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn reassociate_handles_deep_grp_nesting() {
        let mem = Bump::new();
        let mut obj: &DenullObj = term(&mem, "x");
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Grp(obj));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = reassociate(&mem, doc);
        let DenullDoc::Line(result) = out else {
            panic!("expected a line")
        };
        let mut count = 0usize;
        let mut cur: &DenullObj = result;
        while let DenullObj::Grp(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn reassociate_handles_long_doc_spine() {
        let mem = Bump::new();
        let mut doc: &DenullDoc = mem.alloc(DenullDoc::Eod);
        for _ in 0..DEEP {
            doc = mem.alloc(DenullDoc::Break(term(&mem, "x"), doc));
        }
        let out = reassociate(&mem, doc);
        let mut count = 0usize;
        let mut cur = out;
        while let DenullDoc::Break(_, rest) = cur {
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, DenullDoc::Eod));
        assert_eq!(count, DEEP);
    }
}
