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

use crate::arena::{Arena, Id};
use crate::ir::{Fix, Obj, Term};
use crate::serialize::FixedDoc;

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
    use crate::constructors::{comp, fix, grp, seq, text};
    use crate::ir::TermLeaf;
    use crate::layout::{Break, Layout, Pad};
    use crate::serialize::serialize;

    /// The rebuilt tree of a one-line layout, printed as nested constructor
    /// names over the texts.
    fn shape(layout: &Layout) -> String {
        let fixed = serialize(&layout.nodes, &layout.text);
        let out = resolve_scopes(&fixed);
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        fn obj(out: &RebuildDoc, id: RObjId) -> String {
            match out.objs[id] {
                Obj::Term(t) => term(t),
                Obj::Fix(f) => format!("Fix({})", fix_obj(out, f)),
                Obj::Grp(c) => format!("Grp({})", obj(out, c)),
                Obj::Seq(c) => format!("Seq({})", obj(out, c)),
                Obj::Comp(l, r, _) => format!("Comp({}, {})", obj(out, l), obj(out, r)),
            }
        }
        fn fix_obj(out: &RebuildDoc, id: RFixId) -> String {
            match out.fixes[id] {
                Fix::Term(t) => term(t),
                Fix::Comp(l, r, _) => format!("Comp({}, {})", fix_obj(out, l), fix_obj(out, r)),
            }
        }
        fn term(t: Term) -> String {
            match t.leaf {
                TermLeaf::Text(s) => s.to_string(),
                TermLeaf::Null => "null".to_string(),
            }
        }
        obj(&out, root)
    }

    fn pad(l: Layout, r: Layout) -> Layout {
        comp(l, r, Pad::Padded, Break::Breakable)
    }

    fn fixed(l: Layout, r: Layout) -> Layout {
        comp(l, r, Pad::Padded, Break::Fixed)
    }

    #[test]
    fn nested_scopes_are_rebuilt_as_wrappers() {
        // The tree structure survives the round trip through the item list.
        let layout = pad(grp(pad(text("a"), text("b"))), text("c"));
        assert_eq!(shape(&layout), "Comp(Grp(Comp(a, b)), c)");
        let layout = seq(pad(text("a"), grp(pad(text("b"), text("c")))));
        assert_eq!(shape(&layout), "Seq(Comp(a, Grp(Comp(b, c))))");
    }

    #[test]
    fn scope_widens_to_cover_a_fix_run_that_straddles_its_end() {
        // grp(a + b) !+ c: the fixed composition coalesces b and c into one
        // item, so the grp cannot end between them; it widens to include c.
        let layout = fixed(grp(pad(text("a"), text("b"))), text("c"));
        assert_eq!(shape(&layout), "Grp(Comp(a, Fix(Comp(b, c))))");
    }

    #[test]
    fn straddled_scopes_resolve_seq_outward_and_grp_inward() {
        // seq(a + b) !+ grp(c + d): the item [b c] both closes the seq and
        // opens the grp. The seq's end is handed past the grp, so the seq
        // covers everything and the grp sits inside it.
        let layout = fixed(
            seq(pad(text("a"), text("b"))),
            grp(pad(text("c"), text("d"))),
        );
        assert_eq!(
            shape(&layout),
            "Seq(Comp(a, Grp(Comp(Fix(Comp(b, c)), d))))"
        );
    }

    #[test]
    fn fix_runs_are_right_nested_fixed_compositions() {
        let layout = fix(pad(pad(text("a"), text("b")), text("c")));
        assert_eq!(shape(&layout), "Fix(Comp(a, Comp(b, c)))");
    }
}
