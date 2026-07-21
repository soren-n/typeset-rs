//! Pass 9: DenullDoc → FinalDoc (rescope nest and pack)
//!
//! Strips each term's nest/pack wrappers into a `Prop` list, then factors the
//! common prefix shared by a composition's two operands back out around the
//! composition (rescoping), applying the leftover props to each operand
//! individually. The original expressed the object/fix walks as native-stack
//! tree recursion with `compose`d continuations, aborting on deep inputs.
//!
//! Here the object and fix walks are descend/ascend trampolines over
//! heap-allocated frame stacks; the prop-list helpers (strip, join, wrap) and
//! the doc spine are plain loops.

use crate::compiler::types::{
    DenullDoc, DenullFix, DenullObj, DenullTerm, FinalDoc, FinalDocObj, FinalDocObjFix,
};
use bumpalo::Bump;

#[derive(Debug, Copy, Clone)]
enum Prop {
    Nest,
    Pack(u64),
}

/// A doc-spine element with a tail (terminals are handled separately).
enum DocItem<'b> {
    Empty,
    Break(&'b FinalDocObj<'b>),
}

/// Frames for the object trampoline; ascending value is `(props, obj)`.
enum ObjFrame<'b, 'a> {
    Grp,
    Seq,
    CompLeft {
        right: &'a DenullObj<'a>,
        pad: bool,
    },
    CompRight {
        l_props: Vec<Prop>,
        left1: &'b FinalDocObj<'b>,
        pad: bool,
    },
}

/// Frames for the fix trampoline; ascending value is `(props, fix)`.
enum FixFrame<'b, 'a> {
    CompLeft {
        right: &'a DenullFix<'a>,
        pad: bool,
    },
    CompRight {
        l_props: Vec<Prop>,
        left1: &'b FinalDocObjFix<'b>,
        pad: bool,
    },
}

/// Rescope nest and pack
pub fn rescope<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b FinalDoc<'b> {
    // Walk the linear spine to its terminal, rescoping each object.
    let mut items: Vec<DocItem<'b>> = Vec::new();
    let mut cur = doc;
    let terminal: &'b FinalDoc<'b> = loop {
        match cur {
            DenullDoc::Eod => break mem.alloc(FinalDoc::Eod),
            DenullDoc::Line(obj) => {
                let (props, obj1) = _visit_obj(mem, obj);
                break mem.alloc(FinalDoc::Line(_wrap_props(mem, &props, obj1)));
            }
            DenullDoc::Empty(doc1) => {
                items.push(DocItem::Empty);
                cur = doc1;
            }
            DenullDoc::Break(obj, doc1) => {
                let (props, obj1) = _visit_obj(mem, obj);
                items.push(DocItem::Break(_wrap_props(mem, &props, obj1)));
                cur = doc1;
            }
        }
    };
    let mut result = terminal;
    for item in items.iter().rev() {
        result = match item {
            DocItem::Empty => mem.alloc(FinalDoc::Empty(result)),
            DocItem::Break(obj) => mem.alloc(FinalDoc::Break(obj, result)),
        };
    }
    result
}

/// Rescopes one object, returning its stripped prop prefix and the rescoped
/// object.
fn _visit_obj<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: &'a DenullObj<'a>,
) -> (Vec<Prop>, &'b FinalDocObj<'b>) {
    let mut stack: Vec<ObjFrame<'b, 'a>> = Vec::new();
    let mut cur = obj;
    'machine: loop {
        let mut val: (Vec<Prop>, &'b FinalDocObj<'b>) = loop {
            match cur {
                DenullObj::Term(term) => break _visit_term(mem, term),
                DenullObj::Fix(fix) => {
                    let (props, fix1) = _visit_fix(mem, fix);
                    break (props, mem.alloc(FinalDocObj::Fix(fix1)));
                }
                DenullObj::Grp(obj1) => {
                    stack.push(ObjFrame::Grp);
                    cur = obj1;
                }
                DenullObj::Seq(obj1) => {
                    stack.push(ObjFrame::Seq);
                    cur = obj1;
                }
                DenullObj::Comp(left, right, pad) => {
                    stack.push(ObjFrame::CompLeft { right, pad: *pad });
                    cur = left;
                }
            }
        };
        loop {
            match stack.pop() {
                None => return val,
                Some(ObjFrame::Grp) => val = (val.0, mem.alloc(FinalDocObj::Grp(val.1))),
                Some(ObjFrame::Seq) => val = (val.0, mem.alloc(FinalDocObj::Seq(val.1))),
                Some(ObjFrame::CompLeft { right, pad }) => {
                    stack.push(ObjFrame::CompRight {
                        l_props: val.0,
                        left1: val.1,
                        pad,
                    });
                    cur = right;
                    continue 'machine;
                }
                Some(ObjFrame::CompRight {
                    mut l_props,
                    left1,
                    pad,
                }) => {
                    let (r_props, right1) = val;
                    // Factor the common prop prefix out around the composition;
                    // apply the leftovers to each operand individually. `l_props`
                    // is reused as the common prefix (truncated in place).
                    let k = _common_prefix_len(&l_props, &r_props);
                    let left2 = _wrap_props(mem, &l_props[k..], left1);
                    let right2 = _wrap_props(mem, &r_props[k..], right1);
                    let comp = mem.alloc(FinalDocObj::Comp(left2, right2, pad));
                    l_props.truncate(k);
                    val = (l_props, comp);
                }
            }
        }
    }
}

/// Rescopes a fixed sub-object. A fix composition keeps only its left operand's
/// props (the right operand's are dropped, matching the original).
fn _visit_fix<'b, 'a: 'b>(
    mem: &'b Bump,
    fix: &'a DenullFix<'a>,
) -> (Vec<Prop>, &'b FinalDocObjFix<'b>) {
    let mut stack: Vec<FixFrame<'b, 'a>> = Vec::new();
    let mut cur = fix;
    'machine: loop {
        let mut val: (Vec<Prop>, &'b FinalDocObjFix<'b>) = loop {
            match cur {
                DenullFix::Term(term) => break _visit_fix_term(mem, term),
                DenullFix::Comp(left, right, pad) => {
                    stack.push(FixFrame::CompLeft { right, pad: *pad });
                    cur = left;
                }
            }
        };
        loop {
            match stack.pop() {
                None => return val,
                Some(FixFrame::CompLeft { right, pad }) => {
                    stack.push(FixFrame::CompRight {
                        l_props: val.0,
                        left1: val.1,
                        pad,
                    });
                    cur = right;
                    continue 'machine;
                }
                Some(FixFrame::CompRight {
                    l_props,
                    left1,
                    pad,
                }) => {
                    let (_r_props, right1) = val;
                    val = (l_props, mem.alloc(FinalDocObjFix::Comp(left1, right1, pad)));
                }
            }
        }
    }
}

/// Strips a term chain into its prop prefix (index 0 = outermost) and a text obj.
fn _visit_term<'b, 'a: 'b>(
    mem: &'b Bump,
    term: &'a DenullTerm<'a>,
) -> (Vec<Prop>, &'b FinalDocObj<'b>) {
    let (props, data) = _strip_term(term);
    (props, mem.alloc(FinalDocObj::Text(data)))
}

/// Strips a fix term chain into its prop prefix and a fixed text obj.
fn _visit_fix_term<'b, 'a: 'b>(
    mem: &'b Bump,
    term: &'a DenullTerm<'a>,
) -> (Vec<Prop>, &'b FinalDocObjFix<'b>) {
    let (props, data) = _strip_term(term);
    (props, mem.alloc(FinalDocObjFix::Text(data)))
}

/// Collects a term's nest/pack wrappers (outermost first) and its text data.
fn _strip_term<'a>(term: &'a DenullTerm<'a>) -> (Vec<Prop>, &'a str) {
    let mut props: Vec<Prop> = Vec::new();
    let mut cur = term;
    let data: &'a str = loop {
        match cur {
            DenullTerm::Text(data) => break data,
            DenullTerm::Nest(term1) => {
                props.push(Prop::Nest);
                cur = term1;
            }
            DenullTerm::Pack(index, term1) => {
                props.push(Prop::Pack(*index));
                cur = term1;
            }
        }
    };
    (props, data)
}

/// Length of the common prop prefix of `l` and `r` (matching Nest/Nest or
/// Pack/Pack with the same index).
fn _common_prefix_len(l: &[Prop], r: &[Prop]) -> usize {
    let mut k = 0;
    while k < l.len() && k < r.len() {
        let same = match (l[k], r[k]) {
            (Prop::Nest, Prop::Nest) => true,
            (Prop::Pack(li), Prop::Pack(ri)) => li == ri,
            _ => false,
        };
        if !same {
            break;
        }
        k += 1;
    }
    k
}

/// Wraps a term with its props, index 0 outermost.
fn _wrap_props<'b>(
    mem: &'b Bump,
    props: &[Prop],
    term: &'b FinalDocObj<'b>,
) -> &'b FinalDocObj<'b> {
    // Apply from the tail so the first prop ends up outermost.
    let mut obj = term;
    for prop in props.iter().rev() {
        obj = match prop {
            Prop::Nest => mem.alloc(FinalDocObj::Nest(obj)),
            Prop::Pack(index) => mem.alloc(FinalDocObj::Pack(*index, obj)),
        };
    }
    obj
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    fn nest_text<'b>(mem: &'b Bump, depth: usize, s: &'b str) -> &'b DenullTerm<'b> {
        let mut term: &DenullTerm = mem.alloc(DenullTerm::Text(s));
        for _ in 0..depth {
            term = mem.alloc(DenullTerm::Nest(term));
        }
        term
    }

    fn obj_term<'b>(mem: &'b Bump, term: &'b DenullTerm<'b>) -> &'b DenullObj<'b> {
        mem.alloc(DenullObj::Term(term))
    }

    #[test]
    fn rescope_handles_deep_nest_term() {
        let mem = Bump::new();
        let obj = obj_term(&mem, nest_text(&mem, DEEP, "x"));
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = rescope(&mem, doc);
        let FinalDoc::Line(result) = out else {
            panic!("expected a line")
        };
        // The stripped nests are re-applied around the text.
        let mut count = 0usize;
        let mut cur: &FinalDocObj = result;
        while let FinalDocObj::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert!(matches!(cur, FinalDocObj::Text(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn rescope_factors_deep_shared_nest_prefix() {
        let mem = Bump::new();
        // Both operands share a Nest^DEEP prefix: join lifts all of it out
        // around the composition.
        let comp: &DenullObj = mem.alloc(DenullObj::Comp(
            obj_term(&mem, nest_text(&mem, DEEP, "a")),
            obj_term(&mem, nest_text(&mem, DEEP, "b")),
            false,
        ));
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(comp));
        let out = rescope(&mem, doc);
        let FinalDoc::Line(result) = out else {
            panic!("expected a line")
        };
        let mut count = 0usize;
        let mut cur: &FinalDocObj = result;
        while let FinalDocObj::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        // The common nests wrap a single composition of the bare texts.
        assert_eq!(count, DEEP);
        let FinalDocObj::Comp(left, right, _) = cur else {
            panic!("expected the lifted comp")
        };
        assert!(matches!(left, FinalDocObj::Text(_)));
        assert!(matches!(right, FinalDocObj::Text(_)));
    }

    #[test]
    fn rescope_handles_deep_comp_object() {
        let mem = Bump::new();
        // Right-nested comp of plain terms exercises the object frame stack.
        let mut obj: &DenullObj = obj_term(&mem, mem.alloc(DenullTerm::Text("z")));
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Comp(
                obj_term(&mem, mem.alloc(DenullTerm::Text("y"))),
                obj,
                false,
            ));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = rescope(&mem, doc);
        let FinalDoc::Line(result) = out else {
            panic!("expected a line")
        };
        let mut count = 0usize;
        let mut cur: &FinalDocObj = result;
        while let FinalDocObj::Comp(_left, right, _) = cur {
            count += 1;
            cur = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn rescope_handles_deep_fix_comp() {
        let mem = Bump::new();
        let mut fix: &DenullFix = mem.alloc(DenullFix::Term(mem.alloc(DenullTerm::Text("z"))));
        for _ in 0..DEEP {
            fix = mem.alloc(DenullFix::Comp(
                mem.alloc(DenullFix::Term(mem.alloc(DenullTerm::Text("y")))),
                fix,
                false,
            ));
        }
        let obj: &DenullObj = mem.alloc(DenullObj::Fix(fix));
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = rescope(&mem, doc);
        let FinalDoc::Line(FinalDocObj::Fix(fix1)) = out else {
            panic!("expected a fix object")
        };
        let mut count = 0usize;
        let mut cur: &FinalDocObjFix = fix1;
        while let FinalDocObjFix::Comp(_left, right, _) = cur {
            count += 1;
            cur = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn rescope_handles_long_doc_spine() {
        let mem = Bump::new();
        let mut doc: &DenullDoc = mem.alloc(DenullDoc::Eod);
        for _ in 0..DEEP {
            let obj = obj_term(&mem, mem.alloc(DenullTerm::Text("x")));
            doc = mem.alloc(DenullDoc::Break(obj, doc));
        }
        let out = rescope(&mem, doc);
        let mut count = 0usize;
        let mut cur = out;
        while let FinalDoc::Break(_, rest) = cur {
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, FinalDoc::Eod));
        assert_eq!(count, DEEP);
    }
}
