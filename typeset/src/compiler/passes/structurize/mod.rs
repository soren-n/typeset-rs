//! Pass 5: FixedDoc → RebuildDoc (rebuild with graph structure)
//!
//! The pass runs in three phases, one per submodule:
//! 1. [`graphify`] — FixedDoc → GraphDoc: build the grp/seq scope graph.
//! 2. [`solve`]    — GraphDoc → GraphDoc: resolve the scope graph in place.
//! 3. [`rebuild`]  — GraphDoc → RebuildDoc: rebuild the composition spine.
//!
//! Each composition arrives carrying the scopes that open and close at it (the
//! deltas `serialize` computed). `graphify` replays those to build the scope
//! graph in time linear in the number of scopes, so deeply nested grp/seq is
//! linear rather than the O(n^2) a per-composition full-stack diff would cost.

mod graph;
mod graphify;
mod rebuild;
mod solve;

use crate::compiler::types::{FixedDoc, RebuildDoc};
use bumpalo::Bump;

pub fn structurize<'b, 'a: 'b>(mem: &'b Bump, doc: FixedDoc<'a>) -> RebuildDoc<'b> {
    let doc1 = graphify::graphify(mem, doc);
    let doc2 = solve::solve(mem, doc1);
    rebuild::rebuild(doc2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{
        FixedComp, FixedFix, FixedItem, FixedObj, RebuildFix, RebuildObj, Term,
    };

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration across
    /// all three phases (graphify, solve, rebuild).
    const DEEP: usize = 50_000;

    #[test]
    fn structurize_handles_deep_comp_line() {
        let mem = Bump::new();
        // A single line of many plain compositions (no grp/seq scopes). This
        // path is linear (no scope stacks to carry), so a large depth well past
        // the ~400-level native-recursion overflow threshold stays quick and
        // still proves the phases iterate rather than recurse.
        let depth = 20_000usize;
        let mut obj: &FixedObj = mem.alloc(FixedObj::Last(
            mem.alloc(FixedItem::Term(mem.alloc(Term::Text("z")))),
        ));
        for _ in 0..depth {
            obj = mem.alloc(FixedObj::Next(
                mem.alloc(FixedItem::Term(mem.alloc(Term::Text("y")))),
                mem.alloc(FixedComp {
                    pad: false,
                    opens: &[],
                    closes: &[],
                }),
                obj,
            ));
        }
        let doc: FixedDoc = mem.alloc_slice_copy(&[obj]);
        let out = structurize(&mem, doc);
        // One line, rebuilt as a right-nested composition spine.
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        let mut count = 0usize;
        let mut cur = root;
        while let RebuildObj::Comp(_left, right, _pad) = out.objs[cur as usize] {
            count += 1;
            cur = right;
        }
        assert_eq!(count, depth);
    }

    #[test]
    fn structurize_handles_deep_nest_term() {
        let mem = Bump::new();
        // A deep Nest term passes through graphify/rebuild by borrow.
        let mut term: &Term = mem.alloc(Term::Text("x"));
        for _ in 0..DEEP {
            term = mem.alloc(Term::Nest(term));
        }
        let obj: &FixedObj = mem.alloc(FixedObj::Last(mem.alloc(FixedItem::Term(term))));
        let doc: FixedDoc = mem.alloc_slice_copy(&[obj]);
        let out = structurize(&mem, doc);
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        let RebuildObj::Term(t) = out.objs[root as usize] else {
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

    #[test]
    fn structurize_handles_deep_fix_group() {
        let mem = Bump::new();
        // A deep fixed group exercises the fix walks in graphify/rebuild.
        let mut fix: &FixedFix = mem.alloc(FixedFix::Last(mem.alloc(Term::Text("z"))));
        for _ in 0..DEEP {
            fix = mem.alloc(FixedFix::Next(
                mem.alloc(Term::Text("y")),
                mem.alloc(FixedComp {
                    pad: false,
                    opens: &[],
                    closes: &[],
                }),
                fix,
            ));
        }
        let obj: &FixedObj = mem.alloc(FixedObj::Last(mem.alloc(FixedItem::Fix(fix))));
        let doc: FixedDoc = mem.alloc_slice_copy(&[obj]);
        let out = structurize(&mem, doc);
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        let RebuildObj::Fix(rfix) = out.objs[root as usize] else {
            panic!("expected a fix object")
        };
        let mut count = 0usize;
        let mut cur = rfix;
        while let RebuildFix::Comp(_left, right, _pad) = out.fixes[cur as usize] {
            count += 1;
            cur = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn structurize_handles_long_doc_spine() {
        let mem = Bump::new();
        // Many document rows exercise the doc-spine walks in all three phases.
        let mut objs: Vec<&FixedObj> = Vec::new();
        for _ in 0..DEEP {
            objs.push(mem.alloc(FixedObj::Last(
                mem.alloc(FixedItem::Term(mem.alloc(Term::Text("x")))),
            )));
        }
        let doc: FixedDoc = mem.alloc_slice_copy(&objs);
        let out = structurize(&mem, doc);
        assert_eq!(out.lines.len(), DEEP);
    }
}
