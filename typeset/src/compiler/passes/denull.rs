//! Pass 6: RebuildDoc → DenullDoc (remove null identities)
//!
//! Drops `Null`/empty-text terms, collapsing objects that reduce to nothing.
//! The input is a flat postorder arena (children precede parents), so the
//! object and fix walks are plain forward folds over the arena: by the time a
//! node is visited its children's results are already computed. No frame
//! stacks, and deep documents use no native stack by construction.

use crate::compiler::types::{
    DenullDoc, DenullFix, DenullObj, DenullTerm, RebuildDoc, RebuildFix, RebuildObj, Term,
};
use bumpalo::Bump;

/// Result of denulling an object: nothing survived (`None`); an object
/// survived (`Some`); or everything to the left of a composition was dropped,
/// leaving a surviving object plus the accumulated pad (`NextNone`).
#[derive(Copy, Clone)]
enum ObjRes<'b> {
    None,
    Some(&'b DenullObj<'b>),
    NextNone(bool, &'b DenullObj<'b>),
}

/// Same three-way result for fixed sub-objects.
#[derive(Copy, Clone)]
enum FixRes<'b> {
    None,
    Some(&'b DenullFix<'b>),
    NextNone(bool, &'b DenullFix<'b>),
}

/// A `Term` wrapper, recorded outermost-first while descending.
enum TermWrap {
    Nest,
    Pack(u64),
}

/// One line's outcome after denulling: either the object survived, or it
/// reduced to nothing (an empty line).
enum Kind<'b> {
    Empty,
    Obj(&'b DenullObj<'b>),
}

/// Remove null identities.
pub fn denull<'b, 'a: 'b>(mem: &'b Bump, doc: &RebuildDoc<'a>) -> &'b DenullDoc<'b> {
    // Fold the fixed-object arena bottom-up (forward, children first).
    let mut fix_res: Vec<FixRes<'b>> = Vec::with_capacity(doc.fixes.len());
    for node in &doc.fixes {
        let res = match node {
            RebuildFix::Term(term) => match visit_term(mem, term) {
                None => FixRes::None,
                Some(term1) => FixRes::Some(mem.alloc(DenullFix::Term(term1))),
            },
            RebuildFix::Comp(left, right, l_pad) => {
                match (fix_res[*left as usize], fix_res[*right as usize]) {
                    (FixRes::None, FixRes::None) => FixRes::None,
                    (FixRes::None, FixRes::Some(right1)) => FixRes::NextNone(*l_pad, right1),
                    (FixRes::None, FixRes::NextNone(r_pad, right1)) => {
                        FixRes::NextNone(*l_pad || r_pad, right1)
                    }
                    (FixRes::Some(left1), FixRes::None) => FixRes::Some(left1),
                    (FixRes::Some(left1), FixRes::Some(right1)) => {
                        FixRes::Some(mem.alloc(DenullFix::Comp(left1, right1, *l_pad)))
                    }
                    (FixRes::Some(left1), FixRes::NextNone(r_pad, right1)) => {
                        FixRes::Some(mem.alloc(DenullFix::Comp(left1, right1, *l_pad || r_pad)))
                    }
                    // A composition's left operand never denulls to NextNone.
                    (FixRes::NextNone(..), _) => unreachable!("Invariant"),
                }
            }
        };
        fix_res.push(res);
    }

    // Fold the object arena bottom-up the same way.
    let mut obj_res: Vec<ObjRes<'b>> = Vec::with_capacity(doc.objs.len());
    for node in &doc.objs {
        let res = match node {
            RebuildObj::Term(term) => match visit_term(mem, term) {
                None => ObjRes::None,
                Some(term1) => ObjRes::Some(mem.alloc(DenullObj::Term(term1))),
            },
            RebuildObj::Fix(fix) => match fix_res[*fix as usize] {
                FixRes::None => ObjRes::None,
                FixRes::Some(fix1) | FixRes::NextNone(_, fix1) => {
                    ObjRes::Some(mem.alloc(DenullObj::Fix(fix1)))
                }
            },
            RebuildObj::Grp(obj1) => wrap_obj(mem, obj_res[*obj1 as usize], DenullObj::Grp),
            RebuildObj::Seq(obj1) => wrap_obj(mem, obj_res[*obj1 as usize], DenullObj::Seq),
            RebuildObj::Comp(left, right, l_pad) => {
                match (obj_res[*left as usize], obj_res[*right as usize]) {
                    (ObjRes::None, ObjRes::None) => ObjRes::None,
                    (ObjRes::None, ObjRes::Some(right1)) => ObjRes::NextNone(*l_pad, right1),
                    (ObjRes::None, ObjRes::NextNone(r_pad, right1)) => {
                        ObjRes::NextNone(*l_pad || r_pad, right1)
                    }
                    (ObjRes::Some(left1), ObjRes::None) => ObjRes::Some(left1),
                    (ObjRes::Some(left1), ObjRes::Some(right1)) => {
                        ObjRes::Some(mem.alloc(DenullObj::Comp(left1, right1, *l_pad)))
                    }
                    (ObjRes::Some(left1), ObjRes::NextNone(r_pad, right1)) => {
                        ObjRes::Some(mem.alloc(DenullObj::Comp(left1, right1, *l_pad || r_pad)))
                    }
                    // A composition's left operand never denulls to NextNone.
                    (ObjRes::NextNone(..), _) => unreachable!("Invariant"),
                }
            }
        };
        obj_res.push(res);
    }

    // Fold the line outcomes from the tail: a surviving object becomes a
    // Break (or a Line if it is the last), an empty object becomes an Empty
    // wrapper (or Eod if it is the last).
    let kinds = doc.lines.iter().map(|&root| match obj_res[root as usize] {
        ObjRes::None => Kind::Empty,
        ObjRes::Some(obj1) | ObjRes::NextNone(_, obj1) => Kind::Obj(obj1),
    });
    let mut acc: Option<&'b DenullDoc<'b>> = None;
    for kind in kinds.collect::<Vec<_>>().into_iter().rev() {
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

/// Wraps a denulled object in a `Grp` or `Seq` (whichever `ctor` builds),
/// propagating the "nothing survived" result unchanged. A surviving `NextNone`
/// collapses to `Some` — the dropped-left pad is discarded at a wrapper.
fn wrap_obj<'b>(
    mem: &'b Bump,
    val: ObjRes<'b>,
    ctor: fn(&'b DenullObj<'b>) -> DenullObj<'b>,
) -> ObjRes<'b> {
    match val {
        ObjRes::None => ObjRes::None,
        ObjRes::Some(obj) | ObjRes::NextNone(_, obj) => ObjRes::Some(mem.alloc(ctor(obj))),
    }
}

/// Denulls a term chain: `Null` and empty text vanish; `Nest`/`Pack` wrappers
/// are preserved over a surviving term and dropped over nothing.
fn visit_term<'b, 'a: 'b>(mem: &'b Bump, term: &'a Term<'a>) -> Option<&'b DenullTerm<'b>> {
    let mut wraps: Vec<TermWrap> = Vec::new();
    let mut cur = term;
    let mut val: Option<&'b DenullTerm<'b>> = loop {
        match cur {
            Term::Null => break None,
            Term::Text(data) => {
                break if data.is_empty() {
                    None
                } else {
                    Some(mem.alloc(DenullTerm::Text(data)))
                };
            }
            Term::Nest(term1) => {
                wraps.push(TermWrap::Nest);
                cur = term1;
            }
            Term::Pack(index, term1) => {
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
    use crate::compiler::types::{RFixId, RObjId};

    /// Far past where a native-stack recursion could survive; with flat arenas
    /// the folds are plain loops, so this now guards the row/term walks only.
    const DEEP: usize = 50_000;

    fn push_obj<'a>(objs: &mut Vec<RebuildObj<'a>>, node: RebuildObj<'a>) -> RObjId {
        let id = objs.len() as RObjId;
        objs.push(node);
        id
    }

    fn push_fix<'a>(fixes: &mut Vec<RebuildFix<'a>>, node: RebuildFix<'a>) -> RFixId {
        let id = fixes.len() as RFixId;
        fixes.push(node);
        id
    }

    #[test]
    fn denull_handles_deep_comp_object() {
        let mem = Bump::new();
        // Right-nested Comp chain: Comp(Term, Comp(Term, ... Term)). Each left
        // operand is a surviving Term, so the whole object survives.
        let mut objs: Vec<RebuildObj> = Vec::new();
        let mut cur = push_obj(&mut objs, RebuildObj::Term(mem.alloc(Term::Text("z"))));
        for _ in 0..DEEP {
            let left = push_obj(&mut objs, RebuildObj::Term(mem.alloc(Term::Text("y"))));
            cur = push_obj(&mut objs, RebuildObj::Comp(left, cur, false));
        }
        let doc = RebuildDoc {
            lines: vec![cur],
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&mem, &doc);
        // Count the surviving comps in the single line.
        let obj_out = match out {
            DenullDoc::Line(o) => *o,
            _ => panic!("expected a single line"),
        };
        let mut count = 0usize;
        let mut walk = obj_out;
        while let DenullObj::Comp(_left, right, _pad) = walk {
            count += 1;
            walk = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn denull_handles_deep_nest_term() {
        let mem = Bump::new();
        let mut term: &Term = mem.alloc(Term::Text("x"));
        for _ in 0..DEEP {
            term = mem.alloc(Term::Nest(term));
        }
        let mut objs: Vec<RebuildObj> = Vec::new();
        let root = push_obj(&mut objs, RebuildObj::Term(term));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&mem, &doc);
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
    fn denull_drops_null_left_and_merges_pads() {
        let mem = Bump::new();
        // Comp(Null, Comp(Null, Text, pad=true), pad=false): the nulls vanish
        // and the pads merge onto the surviving text via NextNone.
        let mut objs: Vec<RebuildObj> = Vec::new();
        let n1 = push_obj(&mut objs, RebuildObj::Term(mem.alloc(Term::Null)));
        let t = push_obj(&mut objs, RebuildObj::Term(mem.alloc(Term::Text("x"))));
        let inner = push_obj(&mut objs, RebuildObj::Comp(n1, t, true));
        let a = push_obj(&mut objs, RebuildObj::Term(mem.alloc(Term::Text("a"))));
        let root = push_obj(&mut objs, RebuildObj::Comp(a, inner, false));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&mem, &doc);
        // The dropped null's pad (true) survives onto the comp.
        let DenullDoc::Line(DenullObj::Comp(_, _, pad)) = out else {
            panic!("expected a comp line")
        };
        assert!(pad, "the dropped left's pad must merge into the comp");
    }

    #[test]
    fn denull_handles_long_doc_spine() {
        let mem = Bump::new();
        let mut objs: Vec<RebuildObj> = Vec::new();
        let mut lines: Vec<RObjId> = Vec::new();
        for _ in 0..DEEP {
            lines.push(push_obj(
                &mut objs,
                RebuildObj::Term(mem.alloc(Term::Text("x"))),
            ));
        }
        let doc = RebuildDoc {
            lines,
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&mem, &doc);
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

    #[test]
    fn denull_fix_arena_folds_bottom_up() {
        let mem = Bump::new();
        // Fix(Comp(Text "a", Text "", pad=true)): the empty right vanishes and
        // the fix survives as its left.
        let mut objs: Vec<RebuildObj> = Vec::new();
        let mut fixes: Vec<RebuildFix> = Vec::new();
        let fa = push_fix(&mut fixes, RebuildFix::Term(mem.alloc(Term::Text("a"))));
        let fe = push_fix(&mut fixes, RebuildFix::Term(mem.alloc(Term::Text(""))));
        let fc = push_fix(&mut fixes, RebuildFix::Comp(fa, fe, true));
        let root = push_obj(&mut objs, RebuildObj::Fix(fc));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes,
        };
        let out = denull(&mem, &doc);
        let DenullDoc::Line(DenullObj::Fix(DenullFix::Term(DenullTerm::Text(data)))) = out else {
            panic!("expected the fix to survive as its left term")
        };
        assert_eq!(*data, "a");
    }
}
