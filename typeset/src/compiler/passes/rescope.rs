//! Pass 9 (final): DenullDoc → Doc (rescope nest and pack, into the heap)
//!
//! Strips each term's nest/pack wrappers into a `Vec<Prop>`, then factors the
//! common prefix shared by a composition's two operands back out around the
//! composition (rescoping), applying the leftover props to each operand
//! individually. The original expressed the object/fix walks as native-stack
//! tree recursion with `compose`d continuations, aborting on deep inputs.
//!
//! This is the last pass, so it builds the owned heap [`Doc`] directly rather
//! than an arena IR that a separate heap-conversion pass would then copy. The
//! object and fix walks are descend/ascend trampolines over heap-allocated
//! frame stacks; the prop helpers (`strip_term`, `common_prefix_len`,
//! `wrap_props`) and the doc spine are plain loops. Every traversal is
//! iterative, so a deeply nested layout lowers with a constant native stack.

use crate::compiler::types::{DenullDoc, DenullFix, DenullObj, DenullTerm, Doc, DocObj, DocObjFix};

#[derive(Debug, Copy, Clone)]
enum Prop {
    Nest,
    Pack(u64),
}

/// Frames for the object trampoline; ascending value is `(props, obj)`.
enum ObjFrame<'a> {
    Grp,
    Seq,
    CompLeft {
        right: &'a DenullObj<'a>,
        pad: bool,
    },
    CompRight {
        l_props: Vec<Prop>,
        left1: Box<DocObj>,
        pad: bool,
    },
}

/// Frames for the fix trampoline; ascending value is `(props, fix)`.
enum FixFrame<'a> {
    CompLeft {
        right: &'a DenullFix<'a>,
        pad: bool,
    },
    CompRight {
        l_props: Vec<Prop>,
        left1: Box<DocObjFix>,
        pad: bool,
    },
}

/// Rescope nest and pack, lowering the arena `DenullDoc` into the heap `Doc`.
pub fn rescope(doc: &DenullDoc) -> Box<Doc> {
    // Walk the linear DenullDoc spine into a Vec, rescoping each line's object,
    // then fold from the tail so each spine node wraps the already-built
    // remainder. Both halves are plain loops, so a long document uses no native
    // stack.
    enum Row {
        Empty,
        Break(Box<DocObj>),
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut cur = doc;
    let terminal: Box<Doc> = loop {
        match cur {
            DenullDoc::Eod => break Box::new(Doc::Eod),
            DenullDoc::Line(obj) => break Box::new(Doc::Line(finish_obj(obj))),
            DenullDoc::Empty(doc1) => {
                rows.push(Row::Empty);
                cur = doc1;
            }
            DenullDoc::Break(obj, doc1) => {
                rows.push(Row::Break(finish_obj(obj)));
                cur = doc1;
            }
        }
    };
    let mut acc = terminal;
    for row in rows.into_iter().rev() {
        acc = match row {
            Row::Empty => Box::new(Doc::Empty(acc)),
            Row::Break(obj) => Box::new(Doc::Break(obj, acc)),
        };
    }
    acc
}

/// Rescopes one line's object and re-applies its stripped prop prefix.
fn finish_obj(obj: &DenullObj) -> Box<DocObj> {
    let (props, obj1) = visit_obj(obj);
    wrap_props(&props, obj1)
}

/// Rescopes one object, returning its stripped prop prefix and the rescoped
/// object.
fn visit_obj<'a>(obj: &'a DenullObj<'a>) -> (Vec<Prop>, Box<DocObj>) {
    let mut stack: Vec<ObjFrame<'a>> = Vec::new();
    let mut cur = obj;
    'machine: loop {
        let mut val: (Vec<Prop>, Box<DocObj>) = loop {
            match cur {
                DenullObj::Term(term) => break visit_term(term),
                DenullObj::Fix(fix) => {
                    let (props, fix1) = visit_fix(fix);
                    break (props, Box::new(DocObj::Fix(fix1)));
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
                Some(ObjFrame::Grp) => val = (val.0, Box::new(DocObj::Grp(val.1))),
                Some(ObjFrame::Seq) => val = (val.0, Box::new(DocObj::Seq(val.1))),
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
                    let k = common_prefix_len(&l_props, &r_props);
                    let left2 = wrap_props(&l_props[k..], left1);
                    let right2 = wrap_props(&r_props[k..], right1);
                    let comp = Box::new(DocObj::Comp(left2, right2, pad));
                    l_props.truncate(k);
                    val = (l_props, comp);
                }
            }
        }
    }
}

/// Rescopes a fixed sub-object. A fix composition keeps only its left operand's
/// props (the right operand's are dropped, matching the original).
fn visit_fix<'a>(fix: &'a DenullFix<'a>) -> (Vec<Prop>, Box<DocObjFix>) {
    let mut stack: Vec<FixFrame<'a>> = Vec::new();
    let mut cur = fix;
    'machine: loop {
        let mut val: (Vec<Prop>, Box<DocObjFix>) = loop {
            match cur {
                DenullFix::Term(term) => break visit_fix_term(term),
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
                    val = (l_props, Box::new(DocObjFix::Comp(left1, right1, pad)));
                }
            }
        }
    }
}

/// Strips a term chain into its prop prefix (index 0 = outermost) and a text obj.
fn visit_term(term: &DenullTerm) -> (Vec<Prop>, Box<DocObj>) {
    let (props, data) = strip_term(term);
    (props, Box::new(DocObj::Text(data.to_string())))
}

/// Strips a fix term chain into its prop prefix and a fixed text obj.
fn visit_fix_term(term: &DenullTerm) -> (Vec<Prop>, Box<DocObjFix>) {
    let (props, data) = strip_term(term);
    (props, Box::new(DocObjFix::Text(data.to_string())))
}

/// Collects a term's nest/pack wrappers (outermost first) and its text data.
fn strip_term<'a>(term: &'a DenullTerm<'a>) -> (Vec<Prop>, &'a str) {
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
fn common_prefix_len(l: &[Prop], r: &[Prop]) -> usize {
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
fn wrap_props(props: &[Prop], term: Box<DocObj>) -> Box<DocObj> {
    // Apply from the tail so the first prop ends up outermost.
    let mut obj = term;
    for prop in props.iter().rev() {
        obj = match prop {
            Prop::Nest => Box::new(DocObj::Nest(obj)),
            Prop::Pack(index) => Box::new(DocObj::Pack(*index, obj)),
        };
    }
    obj
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;

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
        let out = rescope(doc);
        let Doc::Line(result) = &*out else {
            panic!("expected a line")
        };
        // The stripped nests are re-applied around the text.
        let mut count = 0usize;
        let mut cur: &DocObj = result;
        while let DocObj::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert!(matches!(cur, DocObj::Text(_)));
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
        let out = rescope(doc);
        let Doc::Line(result) = &*out else {
            panic!("expected a line")
        };
        let mut count = 0usize;
        let mut cur: &DocObj = result;
        while let DocObj::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        // The common nests wrap a single composition of the bare texts.
        assert_eq!(count, DEEP);
        let DocObj::Comp(left, right, _) = cur else {
            panic!("expected the lifted comp")
        };
        assert!(matches!(&**left, DocObj::Text(_)));
        assert!(matches!(&**right, DocObj::Text(_)));
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
        let out = rescope(doc);
        let Doc::Line(result) = &*out else {
            panic!("expected a line")
        };
        let mut count = 0usize;
        let mut cur: &DocObj = result;
        while let DocObj::Comp(_left, right, _) = cur {
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
        let out = rescope(doc);
        let Doc::Line(result) = &*out else {
            panic!("expected a line")
        };
        let DocObj::Fix(fix1) = &**result else {
            panic!("expected a fix object")
        };
        let mut count = 0usize;
        let mut cur: &DocObjFix = fix1;
        while let DocObjFix::Comp(_left, right, _) = cur {
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
        let out = rescope(doc);
        let mut count = 0usize;
        let mut cur: &Doc = &out;
        while let Doc::Break(_, rest) = cur {
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, Doc::Eod));
        assert_eq!(count, DEEP);
    }
}
