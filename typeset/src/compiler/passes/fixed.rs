//! Pass 4: LinearDoc → FixedDoc (coalesce fixed comps)
//!
//! LinearDoc is a flat slice, and LinearObj (term/comp chain), LinearComp and
//! Term are linear structures, so the original CPS recursion becomes plain
//! iteration. The one piece of state is a run-length grouping: a maximal run
//! of terms connected by fixed compositions is coalesced into a single
//! `FixedItem::Fix`. This keeps the pass off the native stack, which the
//! recursive/continuation version could exhaust on deep inputs.

use super::term_chain::map_term_chain;
use crate::compiler::types::{
    FixedComp, FixedDoc, FixedFix, FixedItem, FixedObj, LinearComp, LinearDoc, LinearObj, Term,
};
use bumpalo::Bump;

pub fn fixed<'b, 'a: 'b>(mem: &'b Bump, doc: LinearDoc<'a>) -> FixedDoc<'b> {
    // Walk the linear spine, coalescing each object into a flat slice of lines
    // in document order.
    let mut objs: Vec<&'b FixedObj<'b>> = Vec::new();
    for obj in doc {
        objs.push(visit_obj(mem, obj));
    }
    mem.alloc_slice_copy(&objs)
}

/// Coalesces one object's term/comp chain. Terms connected by fixed
/// compositions are grouped into a `FixedItem::Fix`; the remaining
/// (non-fixed) compositions separate the resulting items.
fn visit_obj<'b, 'a: 'b>(mem: &'b Bump, obj: &'a LinearObj<'a>) -> &'b FixedObj<'b> {
    // The resulting items and the non-fixed comps that separate them.
    let mut items: Vec<&'b FixedItem<'b>> = Vec::new();
    let mut seps: Vec<&'b FixedComp<'b>> = Vec::new();
    // The (term, fixed-comp) pairs of the fix run currently being built.
    let mut fix_run: Vec<(&'b Term<'b>, &'b FixedComp<'b>)> = Vec::new();
    let mut in_fix = false;

    let mut cur = obj;
    loop {
        match cur {
            LinearObj::Next(term, comp, obj1) => {
                let term1 = map_term_chain(mem, *term);
                let (is_fixed, comp1) = visit_comp(mem, comp);
                if is_fixed {
                    // A fixed composition: extend (or start) the current run.
                    fix_run.push((term1, comp1));
                    in_fix = true;
                } else if in_fix {
                    // A non-fixed composition closes the run; term1 is its last
                    // term, comp1 becomes the object-level separator.
                    let fix = build_fix(mem, &fix_run, term1);
                    items.push(mem.alloc(FixedItem::Fix(fix)));
                    seps.push(comp1);
                    fix_run.clear();
                    in_fix = false;
                } else {
                    // A plain term separated by a non-fixed composition.
                    items.push(mem.alloc(FixedItem::Term(term1)));
                    seps.push(comp1);
                }
                cur = obj1;
            }
            LinearObj::Last(term) => {
                let term1 = map_term_chain(mem, *term);
                if in_fix {
                    let fix = build_fix(mem, &fix_run, term1);
                    items.push(mem.alloc(FixedItem::Fix(fix)));
                } else {
                    items.push(mem.alloc(FixedItem::Term(term1)));
                }
                break;
            }
        }
    }

    // Fold the items and separators into a FixedObj. Every LinearObj ends in a
    // Last, which pushes an item, so `items` is non-empty and `split_last`
    // yields the trailing item plus the `seps.len()` leading items — making the
    // "at least one item" invariant explicit instead of a bare `len() - 1`.
    let (&last_item, init_items) = items
        .split_last()
        .expect("every LinearObj ends in Last, so there is at least one item");
    let mut fobj: &'b FixedObj<'b> = mem.alloc(FixedObj::Last(last_item));
    for (&item, &sep) in init_items.iter().zip(seps.iter()).rev() {
        fobj = mem.alloc(FixedObj::Next(item, sep, fobj));
    }
    fobj
}

/// Builds a fix group: `Next(t0, c0, Next(t1, c1, ... Last(last_term)))`, where
/// `run` holds the `(term, fixed-comp)` pairs in order and `last_term` is the
/// group's final term.
fn build_fix<'b>(
    mem: &'b Bump,
    run: &[(&'b Term<'b>, &'b FixedComp<'b>)],
    last_term: &'b Term<'b>,
) -> &'b FixedFix<'b> {
    let mut fix: &'b FixedFix<'b> = mem.alloc(FixedFix::Last(last_term));
    for (term, comp) in run.iter().rev() {
        fix = mem.alloc(FixedFix::Next(term, comp, fix));
    }
    fix
}

/// Maps a comp to its `FixedComp`, reporting whether its composition is fixed.
/// The scope delta slices pass through by borrow (`Copy`, outlive this arena).
fn visit_comp<'b, 'a: 'b>(mem: &'b Bump, comp: &'a LinearComp<'a>) -> (bool, &'b FixedComp<'b>) {
    let LinearComp {
        attr,
        opens,
        closes,
    } = comp;
    (
        attr.fix,
        mem.alloc(FixedComp {
            pad: attr.pad,
            opens,
            closes,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{Attr, Term};

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    fn linear_obj_chain<'b>(mem: &'b Bump, len: usize, fix: bool) -> &'b LinearObj<'b> {
        let attr = Attr { pad: false, fix };
        let mut obj: &LinearObj = mem.alloc(LinearObj::Last(mem.alloc(Term::Text("z"))));
        for _ in 0..len {
            obj = mem.alloc(LinearObj::Next(
                mem.alloc(Term::Text("y")),
                mem.alloc(LinearComp {
                    attr,
                    opens: &[],
                    closes: &[],
                }),
                obj,
            ));
        }
        obj
    }

    #[test]
    fn fixed_coalesces_deep_fixed_run() {
        let mem = Bump::new();
        let obj = linear_obj_chain(&mem, DEEP, true);
        let doc: LinearDoc = mem.alloc_slice_copy(&[obj]);
        let out = fixed(&mem, doc);
        // All comps fixed: the whole object collapses to one Fix item.
        let [fobj] = out else {
            panic!("expected a single line")
        };
        let FixedObj::Last(FixedItem::Fix(fix)) = fobj else {
            panic!("expected a single Fix item")
        };
        let mut count = 0usize;
        let mut cur: &FixedFix = fix;
        while let FixedFix::Next(_, _, rest) = cur {
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, FixedFix::Last(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn fixed_handles_deep_nonfixed_run() {
        let mem = Bump::new();
        let obj = linear_obj_chain(&mem, DEEP, false);
        let doc: LinearDoc = mem.alloc_slice_copy(&[obj]);
        let out = fixed(&mem, doc);
        // No fixed comps: DEEP Next items then a Last, all plain Terms.
        let [fobj] = out else {
            panic!("expected a single line")
        };
        let mut count = 0usize;
        let mut cur: &FixedObj = fobj;
        while let FixedObj::Next(item, _, rest) = cur {
            assert!(matches!(item, FixedItem::Term(_)));
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, FixedObj::Last(FixedItem::Term(_))));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn fixed_handles_long_doc_spine() {
        let mem = Bump::new();
        let mut objs: Vec<&LinearObj> = Vec::new();
        for _ in 0..DEEP {
            objs.push(mem.alloc(LinearObj::Last(mem.alloc(Term::Text("x")))));
        }
        let doc: LinearDoc = mem.alloc_slice_copy(&objs);
        let out = fixed(&mem, doc);
        assert_eq!(out.len(), DEEP);
    }

    #[test]
    fn fixed_handles_deep_term_chain() {
        let mem = Bump::new();
        let mut term: &Term = mem.alloc(Term::Text("x"));
        for _ in 0..DEEP {
            term = mem.alloc(Term::Nest(term));
        }
        let obj: &LinearObj = mem.alloc(LinearObj::Last(term));
        let doc: LinearDoc = mem.alloc_slice_copy(&[obj]);
        let out = fixed(&mem, doc);
        let [FixedObj::Last(FixedItem::Term(t))] = out else {
            panic!("expected a single term")
        };
        let mut count = 0usize;
        let mut cur: &Term = t;
        while let Term::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }
}
