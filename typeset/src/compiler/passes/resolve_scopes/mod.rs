//! resolve_scopes: FixedDoc → RebuildDoc (rebuild with graph structure)
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

use crate::compiler::types::{FixedDoc, RebuildDoc, Scope};

pub fn resolve_scopes<'a>(doc: &FixedDoc<'a>, scopes: &[Scope]) -> RebuildDoc<'a> {
    let mut graph = graphify::graphify(doc, scopes);
    solve::solve(&mut graph);
    rebuild::rebuild(&graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{
        FixRun, FixedComp, FixedItem, FixedLine, NO_PATH, PathNode, Prop, RebuildFix, RebuildObj,
        ScopeRange, Term, TermLeaf,
    };

    fn text_term(text: &'static str) -> Term<'static> {
        Term {
            path: NO_PATH,
            leaf: TermLeaf::Text(text),
        }
    }

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration across
    /// all three phases (graphify, solve, rebuild).
    const DEEP: usize = 50_000;

    fn sep() -> FixedComp {
        FixedComp {
            pad: false,
            opens: ScopeRange { start: 0, end: 0 },
            closes: ScopeRange { start: 0, end: 0 },
        }
    }

    #[test]
    fn resolve_scopes_handles_deep_comp_line() {
        // A single line of many plain compositions (no grp/seq scopes). This
        // path is linear (no scope stacks to carry), so a large depth well past
        // the ~400-level native-recursion overflow threshold stays quick and
        // still proves the phases iterate rather than recurse.
        let depth = 20_000usize;
        let mut items: Vec<FixedItem> = Vec::new();
        let mut seps: Vec<FixedComp> = Vec::new();
        for _ in 0..depth {
            items.push(FixedItem::Term(text_term("y")));
            seps.push(sep());
        }
        items.push(FixedItem::Term(text_term("z")));
        let doc = FixedDoc {
            lines: vec![FixedLine { items, seps }],
        };
        let out = resolve_scopes(&doc, &[]);
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
    fn resolve_scopes_handles_deep_nest_term() {
        // A deep Nest path passes through graphify/rebuild by value.
        let mut paths: Vec<PathNode> = Vec::new();
        let mut path = NO_PATH;
        for _ in 0..DEEP {
            let id = paths.len() as u32;
            paths.push(PathNode {
                prop: Prop::Nest,
                parent: path,
            });
            path = id;
        }
        let term = Term {
            path,
            leaf: TermLeaf::Text("x"),
        };
        let doc = FixedDoc {
            lines: vec![FixedLine {
                items: vec![FixedItem::Term(term)],
                seps: Vec::new(),
            }],
        };
        let out = resolve_scopes(&doc, &[]);
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        let RebuildObj::Term(t) = out.objs[root as usize] else {
            panic!("expected a single term")
        };
        let mut count = 0usize;
        let mut cur = t.path;
        while cur != NO_PATH {
            assert!(matches!(paths[cur as usize].prop, Prop::Nest));
            count += 1;
            cur = paths[cur as usize].parent;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn resolve_scopes_handles_deep_fix_group() {
        // A deep fixed run exercises the fix walks in graphify/rebuild.
        let mut terms: Vec<Term> = Vec::new();
        let mut run_seps: Vec<FixedComp> = Vec::new();
        for _ in 0..DEEP {
            terms.push(text_term("y"));
            run_seps.push(sep());
        }
        terms.push(text_term("z"));
        let doc = FixedDoc {
            lines: vec![FixedLine {
                items: vec![FixedItem::Fix(FixRun {
                    terms,
                    seps: run_seps,
                })],
                seps: Vec::new(),
            }],
        };
        let out = resolve_scopes(&doc, &[]);
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
    fn resolve_scopes_handles_long_doc_spine() {
        // Many document rows exercise the doc-spine walks in all three phases.
        let lines: Vec<FixedLine> = (0..DEEP)
            .map(|_| FixedLine {
                items: vec![FixedItem::Term(text_term("x"))],
                seps: Vec::new(),
            })
            .collect();
        let doc = FixedDoc { lines };
        let out = resolve_scopes(&doc, &[]);
        assert_eq!(out.lines.len(), DEEP);
    }
}
