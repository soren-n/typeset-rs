//! Pass 4: FixedDoc → RebuildDoc (rebuild with graph structure)
//!
//! The pass runs in three phases, one per submodule:
//! 1. [`graphify`] — FixedDoc → GraphDoc: build the grp/seq scope graph.
//! 2. [`solve`]    — resolve the scope graph in place.
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

pub fn structurize<'a>(doc: &FixedDoc<'a>) -> RebuildDoc<'a> {
    let mut graph = graphify::graphify(doc);
    solve::solve(&mut graph);
    rebuild::rebuild(&graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{
        FixRun, FixedComp, FixedItem, FixedLine, RebuildFix, RebuildObj, Term,
    };
    use bumpalo::Bump;

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration across
    /// all three phases (graphify, solve, rebuild).
    const DEEP: usize = 50_000;

    fn sep<'a>() -> FixedComp<'a> {
        FixedComp {
            pad: false,
            opens: &[],
            closes: &[],
        }
    }

    #[test]
    fn structurize_handles_deep_comp_line() {
        let mem = Bump::new();
        // A single line of many plain compositions (no grp/seq scopes). This
        // path is linear (no scope stacks to carry), so a large depth well past
        // the ~400-level native-recursion overflow threshold stays quick and
        // still proves the phases iterate rather than recurse.
        let depth = 20_000usize;
        let mut items: Vec<FixedItem> = Vec::new();
        let mut seps: Vec<FixedComp> = Vec::new();
        for _ in 0..depth {
            items.push(FixedItem::Term(mem.alloc(Term::Text("y"))));
            seps.push(sep());
        }
        items.push(FixedItem::Term(mem.alloc(Term::Text("z"))));
        let doc = FixedDoc {
            lines: vec![FixedLine { items, seps }],
        };
        let out = structurize(&doc);
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
        let doc = FixedDoc {
            lines: vec![FixedLine {
                items: vec![FixedItem::Term(term)],
                seps: Vec::new(),
            }],
        };
        let out = structurize(&doc);
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
        // A deep fixed run exercises the fix walks in graphify/rebuild.
        let mut terms: Vec<&Term> = Vec::new();
        let mut run_seps: Vec<FixedComp> = Vec::new();
        for _ in 0..DEEP {
            terms.push(mem.alloc(Term::Text("y")));
            run_seps.push(sep());
        }
        terms.push(mem.alloc(Term::Text("z")));
        let doc = FixedDoc {
            lines: vec![FixedLine {
                items: vec![FixedItem::Fix(FixRun {
                    terms,
                    seps: run_seps,
                })],
                seps: Vec::new(),
            }],
        };
        let out = structurize(&doc);
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
        let lines: Vec<FixedLine> = (0..DEEP)
            .map(|_| FixedLine {
                items: vec![FixedItem::Term(mem.alloc(Term::Text("x")))],
                seps: Vec::new(),
            })
            .collect();
        let doc = FixedDoc { lines };
        let out = structurize(&doc);
        assert_eq!(out.lines.len(), DEEP);
    }
}
