//! Pass 5: FixedDoc → RebuildDoc (rebuild with graph structure)
//!
//! The pass runs in three phases, one per submodule:
//! 1. [`graphify`] — FixedDoc → GraphDoc: build the grp/seq scope graph.
//! 2. [`solve`]    — GraphDoc → GraphDoc: resolve the scope graph in place.
//! 3. [`rebuild`]  — GraphDoc → RebuildDoc: rebuild the composition spine.

mod graphify;
mod rebuild;
mod solve;

use crate::compiler::types::{FixedDoc, RebuildDoc};
use bumpalo::Bump;

pub fn structurize<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b RebuildDoc<'b> {
    let doc1 = graphify::graphify(mem, doc);
    let doc2 = solve::solve(mem, doc1);
    rebuild::rebuild(mem, doc2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{
        FixedComp, FixedFix, FixedItem, FixedObj, FixedTerm, RebuildFix, RebuildObj, RebuildTerm,
    };

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration across
    /// all three phases (graphify, solve, rebuild).
    const DEEP: usize = 50_000;

    #[test]
    fn structurize_handles_deep_comp_line() {
        let mem = Bump::new();
        // A single line of many plain compositions (no grp/seq scopes). Indexed
        // node access in solve/rebuild makes this path O(n^2), so use a depth
        // that is well past the ~400-level overflow threshold but still quick.
        let depth = 20_000usize;
        let mut obj: &FixedObj = mem.alloc(FixedObj::Last(
            mem.alloc(FixedItem::Term(mem.alloc(FixedTerm::Text("z")))),
        ));
        for _ in 0..depth {
            obj = mem.alloc(FixedObj::Next(
                mem.alloc(FixedItem::Term(mem.alloc(FixedTerm::Text("y")))),
                mem.alloc(FixedComp::Comp(false)),
                obj,
            ));
        }
        let doc: &FixedDoc = mem.alloc(FixedDoc::Break(obj, mem.alloc(FixedDoc::Eod)));
        let out = structurize(&mem, doc);
        // One line, rebuilt as a right-nested composition spine.
        let RebuildDoc::Break(robj, _) = out else {
            panic!("expected a break")
        };
        let mut count = 0usize;
        let mut cur: &RebuildObj = robj;
        while let RebuildObj::Comp(_left, right, _pad) = cur {
            count += 1;
            cur = right;
        }
        assert_eq!(count, depth);
    }

    #[test]
    fn structurize_handles_deep_nest_term() {
        let mem = Bump::new();
        // A deep Nest term exercises _visit_term at depth in graphify/rebuild.
        let mut term: &FixedTerm = mem.alloc(FixedTerm::Text("x"));
        for _ in 0..DEEP {
            term = mem.alloc(FixedTerm::Nest(term));
        }
        let obj: &FixedObj = mem.alloc(FixedObj::Last(mem.alloc(FixedItem::Term(term))));
        let doc: &FixedDoc = mem.alloc(FixedDoc::Break(obj, mem.alloc(FixedDoc::Eod)));
        let out = structurize(&mem, doc);
        let RebuildDoc::Break(RebuildObj::Term(t), _) = out else {
            panic!("expected a single term")
        };
        let mut count = 0usize;
        let mut cur: &RebuildTerm = t;
        while let RebuildTerm::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn structurize_handles_deep_fix_group() {
        let mem = Bump::new();
        // A deep fixed group exercises the fix trampolines in graphify/rebuild.
        let mut fix: &FixedFix = mem.alloc(FixedFix::Last(mem.alloc(FixedTerm::Text("z"))));
        for _ in 0..DEEP {
            fix = mem.alloc(FixedFix::Next(
                mem.alloc(FixedTerm::Text("y")),
                mem.alloc(FixedComp::Comp(false)),
                fix,
            ));
        }
        let obj: &FixedObj = mem.alloc(FixedObj::Last(mem.alloc(FixedItem::Fix(fix))));
        let doc: &FixedDoc = mem.alloc(FixedDoc::Break(obj, mem.alloc(FixedDoc::Eod)));
        let out = structurize(&mem, doc);
        let RebuildDoc::Break(RebuildObj::Fix(rfix), _) = out else {
            panic!("expected a fix object")
        };
        let mut count = 0usize;
        let mut cur: &RebuildFix = rfix;
        while let RebuildFix::Comp(_left, right, _pad) = cur {
            count += 1;
            cur = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn structurize_handles_long_doc_spine() {
        let mem = Bump::new();
        // Many document rows exercise the doc-spine walks in all three phases.
        let mut doc: &FixedDoc = mem.alloc(FixedDoc::Eod);
        for _ in 0..DEEP {
            let obj: &FixedObj = mem.alloc(FixedObj::Last(
                mem.alloc(FixedItem::Term(mem.alloc(FixedTerm::Text("x")))),
            ));
            doc = mem.alloc(FixedDoc::Break(obj, doc));
        }
        let out = structurize(&mem, doc);
        let mut count = 0usize;
        let mut cur = out;
        while let RebuildDoc::Break(_, rest) = cur {
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, RebuildDoc::Eod));
        assert_eq!(count, DEEP);
    }
}
