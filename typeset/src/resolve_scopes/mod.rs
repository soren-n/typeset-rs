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
use crate::layout::Pad;
use crate::serialize::{FixedDoc, Run};

pub(crate) type ObjId<'a> = Id<Obj<'a>>;

/// An object in a per-line composition tree. A run is a leaf (its terms are
/// ranges into the borrowed `FixedDoc`); a composition's left operand is
/// always a leaf or a wrapper, never another composition.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Obj<'a> {
    Run(Run<'a>),
    Grp(ObjId<'a>),
    Seq(ObjId<'a>),
    Comp(ObjId<'a>, ObjId<'a>, Pad),
}

/// The document rebuilt as one composition tree per line, over runs whose
/// terms still carry their nest/pack paths. The arena is postorder (children
/// precede parents), so consumers fold it with a forward loop.
#[derive(Debug)]
pub(crate) struct RebuildDoc<'a> {
    /// One root object per line, in document order.
    pub(crate) lines: Vec<ObjId<'a>>,
    pub(crate) objs: Arena<Obj<'a>>,
}

pub(crate) fn resolve_scopes<'a>(doc: &FixedDoc<'a>) -> RebuildDoc<'a> {
    let mut graph = graphify::graphify(doc);
    solve::solve(&mut graph);
    rebuild::rebuild(&graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructors::{comp, fix, grp, seq, text};
    use crate::layout::{Break, Layout};
    use crate::serialize::serialize;

    /// The rebuilt tree of a one-line layout, printed as nested constructor
    /// names over the texts; a run of several terms prints as `Fix(a b c)`.
    fn shape(layout: &Layout) -> String {
        let fixed = serialize(&layout.nodes, &layout.text);
        let out = resolve_scopes(&fixed);
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        fn obj<'a>(fixed: &FixedDoc<'a>, out: &RebuildDoc<'a>, id: ObjId<'a>) -> String {
            match out.objs[id] {
                Obj::Run(run) => {
                    let texts: Vec<&str> = run
                        .terms
                        .slice(&fixed.terms)
                        .iter()
                        .map(|t| t.text)
                        .collect();
                    match texts[..] {
                        [one] => one.to_string(),
                        _ => format!("Fix({})", texts.join(" ")),
                    }
                }
                Obj::Grp(c) => format!("Grp({})", obj(fixed, out, c)),
                Obj::Seq(c) => format!("Seq({})", obj(fixed, out, c)),
                Obj::Comp(l, r, _) => {
                    format!("Comp({}, {})", obj(fixed, out, l), obj(fixed, out, r))
                }
            }
        }
        obj(&fixed, &out, root)
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
        assert_eq!(shape(&layout), "Grp(Comp(a, Fix(b c)))");
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
        assert_eq!(shape(&layout), "Seq(Comp(a, Grp(Comp(Fix(b c), d))))");
    }

    #[test]
    fn a_fix_is_one_run() {
        let layout = fix(pad(pad(text("a"), text("b")), text("c")));
        assert_eq!(shape(&layout), "Fix(a b c)");
    }
}
