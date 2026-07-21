//! Pass 6: RebuildDoc → DenullDoc (remove null identities)
//!
//! Drops `Null`/empty-text terms, collapsing objects that reduce to nothing.
//! The original expressed this as a multi-continuation CPS fold (none / some /
//! next_none) recursing on the native stack. Here each visitor instead returns
//! an explicit result enum, and the tree visitors (`visit_obj`, `visit_fix`)
//! run as descend/ascend trampolines over a heap-allocated frame stack so deep
//! inputs no longer overflow.

use crate::compiler::types::{
    DenullDoc, DenullFix, DenullObj, DenullTerm, RebuildDoc, RebuildFix, RebuildObj, RebuildTerm,
};
use bumpalo::Bump;

/// Result of denulling an object: nothing survived (`None`); an object
/// survived (`Some`); or everything to the left of a composition was dropped,
/// leaving a surviving object plus the accumulated pad (`NextNone`).
enum ObjRes<'b> {
    None,
    Some(&'b DenullObj<'b>),
    NextNone(bool, &'b DenullObj<'b>),
}

/// Same three-way result for fixed sub-objects.
enum FixRes<'b> {
    None,
    Some(&'b DenullFix<'b>),
    NextNone(bool, &'b DenullFix<'b>),
}

/// A `RebuildTerm` wrapper, recorded outermost-first while descending.
enum TermWrap {
    Nest,
    Pack(u64),
}

/// Frames for the `visit_obj` trampoline.
enum ObjFrame<'b, 'a> {
    Grp,
    Seq,
    CompLeft {
        right: &'a RebuildObj<'a>,
        l_pad: bool,
    },
    CompRight {
        left: ObjRes<'b>,
        l_pad: bool,
    },
}

/// Frames for the `visit_fix` trampoline.
enum FixFrame<'b, 'a> {
    CompLeft {
        right: &'a RebuildFix<'a>,
        l_pad: bool,
    },
    CompRight {
        left: FixRes<'b>,
        l_pad: bool,
    },
}

/// One line's outcome after denulling: either the object survived, or it
/// reduced to nothing (an empty line).
enum Kind<'b> {
    Empty,
    Obj(&'b DenullObj<'b>),
}

/// Remove null identities
pub fn denull<'b, 'a: 'b>(mem: &'b Bump, doc: &'a RebuildDoc<'a>) -> &'b DenullDoc<'b> {
    // RebuildDoc is a linear spine; walk it, denulling each object.
    let mut kinds: Vec<Kind<'b>> = Vec::new();
    let mut cur = doc;
    loop {
        match cur {
            RebuildDoc::Eod => break,
            RebuildDoc::Break(obj, doc1) => {
                let kind = match visit_obj(mem, obj) {
                    ObjRes::None => Kind::Empty,
                    ObjRes::Some(obj1) | ObjRes::NextNone(_, obj1) => Kind::Obj(obj1),
                };
                kinds.push(kind);
                cur = doc1;
            }
        }
    }

    // Fold the line outcomes from the tail: a surviving object becomes a
    // Break (or a Line if it is the last), an empty object becomes an Empty
    // wrapper (or Eod if it is the last).
    let mut acc: Option<&'b DenullDoc<'b>> = None;
    for kind in kinds.iter().rev() {
        let next: &'b DenullDoc<'b> = match (kind, acc) {
            (Kind::Empty, None) => mem.alloc(DenullDoc::Eod),
            (Kind::Empty, Some(doc2)) => mem.alloc(DenullDoc::Empty(doc2)),
            (Kind::Obj(obj1), None) => mem.alloc(DenullDoc::Line(obj1)),
            (Kind::Obj(obj1), Some(doc2)) => mem.alloc(DenullDoc::Break(obj1, doc2)),
        };
        acc = Some(next);
    }
    acc.unwrap_or_else(|| mem.alloc(DenullDoc::Eod))
}

/// Denulls an object tree. `Comp` branches into left then right; `Grp`/`Seq`
/// wrap their child; `Term`/`Fix` are leaves.
fn visit_obj<'b, 'a: 'b>(mem: &'b Bump, obj: &'a RebuildObj<'a>) -> ObjRes<'b> {
    let mut stack: Vec<ObjFrame<'b, 'a>> = Vec::new();
    let mut cur = obj;
    'descend: loop {
        let mut val: ObjRes<'b> = match cur {
            RebuildObj::Term(term) => match visit_term(mem, term) {
                None => ObjRes::None,
                Some(term1) => ObjRes::Some(mem.alloc(DenullObj::Term(term1))),
            },
            RebuildObj::Fix(fix) => match visit_fix(mem, fix) {
                FixRes::None => ObjRes::None,
                FixRes::Some(fix1) | FixRes::NextNone(_, fix1) => {
                    ObjRes::Some(mem.alloc(DenullObj::Fix(fix1)))
                }
            },
            RebuildObj::Grp(obj1) => {
                stack.push(ObjFrame::Grp);
                cur = obj1;
                continue 'descend;
            }
            RebuildObj::Seq(obj1) => {
                stack.push(ObjFrame::Seq);
                cur = obj1;
                continue 'descend;
            }
            RebuildObj::Comp(left, right, l_pad) => {
                stack.push(ObjFrame::CompLeft {
                    right,
                    l_pad: *l_pad,
                });
                cur = left;
                continue 'descend;
            }
        };
        loop {
            match stack.pop() {
                None => return val,
                Some(ObjFrame::Grp) => {
                    val = match val {
                        ObjRes::None => ObjRes::None,
                        ObjRes::Some(obj2) | ObjRes::NextNone(_, obj2) => {
                            ObjRes::Some(mem.alloc(DenullObj::Grp(obj2)))
                        }
                    };
                }
                Some(ObjFrame::Seq) => {
                    val = match val {
                        ObjRes::None => ObjRes::None,
                        ObjRes::Some(obj2) | ObjRes::NextNone(_, obj2) => {
                            ObjRes::Some(mem.alloc(DenullObj::Seq(obj2)))
                        }
                    };
                }
                Some(ObjFrame::CompLeft { right, l_pad }) => {
                    // A composition's left operand never denulls to NextNone.
                    if matches!(val, ObjRes::NextNone(..)) {
                        unreachable!("Invariant");
                    }
                    stack.push(ObjFrame::CompRight { left: val, l_pad });
                    cur = right;
                    continue 'descend;
                }
                Some(ObjFrame::CompRight { left, l_pad }) => {
                    val = match (left, val) {
                        (ObjRes::None, ObjRes::None) => ObjRes::None,
                        (ObjRes::None, ObjRes::Some(right1)) => ObjRes::NextNone(l_pad, right1),
                        (ObjRes::None, ObjRes::NextNone(r_pad, right1)) => {
                            ObjRes::NextNone(l_pad || r_pad, right1)
                        }
                        (ObjRes::Some(left1), ObjRes::None) => ObjRes::Some(left1),
                        (ObjRes::Some(left1), ObjRes::Some(right1)) => {
                            ObjRes::Some(mem.alloc(DenullObj::Comp(left1, right1, l_pad)))
                        }
                        (ObjRes::Some(left1), ObjRes::NextNone(r_pad, right1)) => {
                            ObjRes::Some(mem.alloc(DenullObj::Comp(left1, right1, l_pad || r_pad)))
                        }
                        (ObjRes::NextNone(..), _) => unreachable!("Invariant"),
                    };
                }
            }
        }
    }
}

/// Denulls a fixed sub-object tree. Mirrors `visit_obj` but only `Comp`
/// branches and `Term` is the sole leaf.
fn visit_fix<'b, 'a: 'b>(mem: &'b Bump, fix: &'a RebuildFix<'a>) -> FixRes<'b> {
    let mut stack: Vec<FixFrame<'b, 'a>> = Vec::new();
    let mut cur = fix;
    'descend: loop {
        let mut val: FixRes<'b> = match cur {
            RebuildFix::Term(term) => match visit_term(mem, term) {
                None => FixRes::None,
                Some(term1) => FixRes::Some(mem.alloc(DenullFix::Term(term1))),
            },
            RebuildFix::Comp(left, right, l_pad) => {
                stack.push(FixFrame::CompLeft {
                    right,
                    l_pad: *l_pad,
                });
                cur = left;
                continue 'descend;
            }
        };
        loop {
            match stack.pop() {
                None => return val,
                Some(FixFrame::CompLeft { right, l_pad }) => {
                    if matches!(val, FixRes::NextNone(..)) {
                        unreachable!("Invariant");
                    }
                    stack.push(FixFrame::CompRight { left: val, l_pad });
                    cur = right;
                    continue 'descend;
                }
                Some(FixFrame::CompRight { left, l_pad }) => {
                    val = match (left, val) {
                        (FixRes::None, FixRes::None) => FixRes::None,
                        (FixRes::None, FixRes::Some(right1)) => FixRes::NextNone(l_pad, right1),
                        (FixRes::None, FixRes::NextNone(r_pad, right1)) => {
                            FixRes::NextNone(l_pad || r_pad, right1)
                        }
                        (FixRes::Some(left1), FixRes::None) => FixRes::Some(left1),
                        (FixRes::Some(left1), FixRes::Some(right1)) => {
                            FixRes::Some(mem.alloc(DenullFix::Comp(left1, right1, l_pad)))
                        }
                        (FixRes::Some(left1), FixRes::NextNone(r_pad, right1)) => {
                            FixRes::Some(mem.alloc(DenullFix::Comp(left1, right1, l_pad || r_pad)))
                        }
                        (FixRes::NextNone(..), _) => unreachable!("Invariant"),
                    };
                }
            }
        }
    }
}

/// Denulls a term chain: `Null` and empty text vanish; `Nest`/`Pack` wrappers
/// are preserved over a surviving term and dropped over nothing.
fn visit_term<'b, 'a: 'b>(mem: &'b Bump, term: &'a RebuildTerm<'a>) -> Option<&'b DenullTerm<'b>> {
    let mut wraps: Vec<TermWrap> = Vec::new();
    let mut cur = term;
    let mut val: Option<&'b DenullTerm<'b>> = loop {
        match cur {
            RebuildTerm::Null => break None,
            RebuildTerm::Text(data) => {
                break if data.is_empty() {
                    None
                } else {
                    Some(mem.alloc(DenullTerm::Text(data)))
                };
            }
            RebuildTerm::Nest(term1) => {
                wraps.push(TermWrap::Nest);
                cur = term1;
            }
            RebuildTerm::Pack(index, term1) => {
                wraps.push(TermWrap::Pack(*index));
                cur = term1;
            }
        }
    };
    while let Some(wrap) = wraps.pop() {
        val = val.map(|term1| -> &'b DenullTerm<'b> {
            match wrap {
                TermWrap::Nest => mem.alloc(DenullTerm::Nest(term1)),
                TermWrap::Pack(index) => mem.alloc(DenullTerm::Pack(index, term1)),
            }
        });
    }
    val
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    #[test]
    fn denull_handles_deep_comp_object() {
        let mem = Bump::new();
        // Right-nested Comp chain: Comp(Term, Comp(Term, ... Term)). Each left
        // operand is a surviving Term, so the whole object survives.
        let mut obj: &RebuildObj = mem.alloc(RebuildObj::Term(mem.alloc(RebuildTerm::Text("z"))));
        for _ in 0..DEEP {
            obj = mem.alloc(RebuildObj::Comp(
                mem.alloc(RebuildObj::Term(mem.alloc(RebuildTerm::Text("y")))),
                obj,
                false,
            ));
        }
        let doc: &RebuildDoc = mem.alloc(RebuildDoc::Break(obj, mem.alloc(RebuildDoc::Eod)));
        let out = denull(&mem, doc);
        // Count the surviving comps in the single line.
        let obj_out = match out {
            DenullDoc::Line(o) => *o,
            _ => panic!("expected a single line"),
        };
        let mut count = 0usize;
        let mut cur = obj_out;
        while let DenullObj::Comp(_left, right, _pad) = cur {
            count += 1;
            cur = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn denull_handles_deep_nest_term() {
        let mem = Bump::new();
        let mut term: &RebuildTerm = mem.alloc(RebuildTerm::Text("x"));
        for _ in 0..DEEP {
            term = mem.alloc(RebuildTerm::Nest(term));
        }
        let obj: &RebuildObj = mem.alloc(RebuildObj::Term(term));
        let doc: &RebuildDoc = mem.alloc(RebuildDoc::Break(obj, mem.alloc(RebuildDoc::Eod)));
        let out = denull(&mem, doc);
        let DenullDoc::Line(DenullObj::Term(t)) = out else {
            panic!("expected a single term line")
        };
        let mut count = 0usize;
        let mut cur: &DenullTerm = t;
        while let DenullTerm::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn denull_handles_long_doc_spine() {
        let mem = Bump::new();
        let mut doc: &RebuildDoc = mem.alloc(RebuildDoc::Eod);
        for _ in 0..DEEP {
            let obj: &RebuildObj = mem.alloc(RebuildObj::Term(mem.alloc(RebuildTerm::Text("x"))));
            doc = mem.alloc(RebuildDoc::Break(obj, doc));
        }
        let out = denull(&mem, doc);
        let mut count = 0usize;
        let mut cur = out;
        loop {
            match cur {
                DenullDoc::Break(_, rest) => {
                    count += 1;
                    cur = rest;
                }
                DenullDoc::Line(_) => {
                    count += 1;
                    break;
                }
                _ => break,
            }
        }
        assert_eq!(count, DEEP);
    }
}
