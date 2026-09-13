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

use super::serialize::FixedDoc;
use crate::compiler::types::{Arena, Fix, Id, Obj, Term};

pub(crate) type RObjId<'a> = Id<Obj<Term<'a>>>;
pub(crate) type RFixId<'a> = Id<Fix<Term<'a>>>;

/// The document rebuilt as one composition tree per line, over terms that
/// still carry their nest/pack paths. Both arenas are postorder (children
/// precede parents), so consumers fold them with a forward loop.
#[derive(Debug)]
pub(crate) struct RebuildDoc<'a> {
    /// One root object per line, in document order.
    pub(crate) lines: Vec<RObjId<'a>>,
    pub(crate) objs: Arena<Obj<Term<'a>>>,
    pub(crate) fixes: Arena<Fix<Term<'a>>>,
}

pub fn resolve_scopes<'a>(doc: &FixedDoc<'a>) -> RebuildDoc<'a> {
    let mut graph = graphify::graphify(doc);
    solve::solve(&mut graph);
    rebuild::rebuild(&graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::passes::serialize::{FixRun, FixedComp, FixedItem, FixedLine};
    use crate::compiler::types::{Pad, PathNode, Prop, Range, TermLeaf};

    /// Wraps `items` and `item_seps` arenas as a single-line [`FixedDoc`] with
    /// no fix runs — the shape most of these tests build.
    fn one_line(items: Vec<FixedItem<'static>>, item_seps: Vec<FixedComp>) -> FixedDoc<'static> {
        let line = FixedLine {
            items: Range::new(0, items.len()),
            seps: Range::new(0, item_seps.len()),
        };
        FixedDoc {
            lines: vec![line],
            items,
            item_seps,
            terms: Vec::new(),
            run_seps: Vec::new(),
            paths: Arena::new(),
            scopes: Vec::new(),
        }
    }

    fn text_term(text: &'static str) -> Term<'static> {
        Term {
            path: None,
            leaf: TermLeaf::Text(text),
        }
    }

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration across
    /// all three phases (graphify, solve, rebuild).
    const DEEP: usize = 50_000;

    fn sep() -> FixedComp {
        FixedComp {
            pad: Pad::Unpadded,
            opens: Range::EMPTY,
            closes: Range::EMPTY,
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
        let mut item_seps: Vec<FixedComp> = Vec::new();
        for _ in 0..depth {
            items.push(FixedItem::Term(text_term("y")));
            item_seps.push(sep());
        }
        items.push(FixedItem::Term(text_term("z")));
        let doc = one_line(items, item_seps);
        let out = resolve_scopes(&doc);
        // One line, rebuilt as a right-nested composition spine.
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        let mut count = 0usize;
        let mut cur = root;
        while let Obj::Comp(_left, right, _pad) = out.objs[cur] {
            count += 1;
            cur = right;
        }
        assert_eq!(count, depth);
    }

    #[test]
    fn resolve_scopes_handles_deep_nest_term() {
        // A deep Nest path passes through graphify/rebuild by value.
        let mut paths: Arena<PathNode> = Arena::new();
        let mut path = None;
        for _ in 0..DEEP {
            path = Some(paths.push(PathNode {
                prop: Prop::Nest,
                parent: path,
            }));
        }
        let term = Term {
            path,
            leaf: TermLeaf::Text("x"),
        };
        let doc = one_line(vec![FixedItem::Term(term)], Vec::new());
        let out = resolve_scopes(&doc);
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        let Obj::Term(t) = out.objs[root] else {
            panic!("expected a single term")
        };
        let mut count = 0usize;
        let mut cur = t.path;
        while let Some(id) = cur {
            assert!(matches!(paths[id].prop, Prop::Nest));
            count += 1;
            cur = paths[id].parent;
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
        // A single line whose one item is a fix run spanning the term and
        // run-separator arenas.
        let run = FixRun {
            terms: Range::new(0, terms.len()),
            seps: Range::new(0, run_seps.len()),
        };
        let doc = FixedDoc {
            lines: vec![FixedLine {
                items: Range::new(0, 1),
                seps: Range::EMPTY,
            }],
            items: vec![FixedItem::Fix(run)],
            item_seps: Vec::new(),
            terms,
            run_seps,
            paths: Arena::new(),
            scopes: Vec::new(),
        };
        let out = resolve_scopes(&doc);
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        let Obj::Fix(rfix) = out.objs[root] else {
            panic!("expected a fix object")
        };
        let mut count = 0usize;
        let mut cur = rfix;
        while let Fix::Comp(_left, right, _pad) = out.fixes[cur] {
            count += 1;
            cur = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn resolve_scopes_handles_long_doc_spine() {
        // Many document rows exercise the doc-spine walks in all three phases.
        let mut items: Vec<FixedItem> = Vec::new();
        let lines: Vec<FixedLine> = (0..DEEP)
            .map(|_| {
                let start = items.len();
                items.push(FixedItem::Term(text_term("x")));
                FixedLine {
                    items: Range::new(start, items.len()),
                    seps: Range::EMPTY,
                }
            })
            .collect();
        let doc = FixedDoc {
            lines,
            items,
            item_seps: Vec::new(),
            terms: Vec::new(),
            run_seps: Vec::new(),
            paths: Arena::new(),
            scopes: Vec::new(),
        };
        let out = resolve_scopes(&doc);
        assert_eq!(out.lines.len(), DEEP);
    }
}
