//! serialize: Layout → `FixedDoc` (lines of runs, scopes as deltas)
//!
//! One left-to-right DFS over the layout arena emits the document as lines of
//! runs with the breakable compositions between them:
//!
//! - A hard line break ends a line. So does every breakable composition
//!   inside a *broken* sequence — a `seq` with a hard line beneath it, which
//!   the constructors mark as such — so its wrapper is dropped and its
//!   breakable compositions become lines. `fix` and `grp` reset that context.
//! - Every item is a run: a maximal sequence of terms joined by fixed
//!   compositions (including every composition under a `fix`). A lone term
//!   is a run of one.
//! - Each `nest`/`pack` descended through pushes one node onto the shared
//!   path arena, so a term is just (path id, text) and sibling leaves share
//!   their path spine.
//! - Each `grp`/`seq` descended through pushes one node onto a parent-linked
//!   scope-chain arena. A composition records how the chain changed since the
//!   previous composition on its line — the scopes that *open* and how many
//!   *close* at it — by diffing the two chains along their shared spine,
//!   which is O(delta), so deeply nested scopes stay linear.
//!
//! Pack indices are DFS pre-order counters, dense so the renderer keys its
//! marks by plain index.

use crate::arena::{Arena, Id, Range, append_range};
use crate::layout::{Attr, Break, LayId, LayoutNode, Pad};

pub(crate) type PathId = Id<PathNode>;

/// One nest/pack wrapper on the DFS path to a leaf. Sibling leaves under the
/// same wrappers share their path spine, so total path storage is O(input
/// tree), not O(leaves × depth).
#[derive(Debug, Copy, Clone)]
pub(crate) struct PathNode {
    pub(crate) prop: Prop,
    /// The enclosing (next-outer) wrapper, `None` at the outermost.
    pub(crate) parent: Option<PathId>,
}

/// A nest/pack wrapper on a term. Pack indices are dense DFS counters.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum Prop {
    Nest,
    Pack(u32),
}

/// A layout leaf: its innermost nest/pack wrapper (a path into the shared
/// path arena, `None` for no wrappers) over its text. The empty layout is the
/// empty text; both vanish in `structure`.
#[derive(Debug, Copy, Clone)]
pub(crate) struct Term<'a> {
    pub(crate) path: Option<PathId>,
    pub(crate) text: &'a str,
}

/// Which of the two breaking disciplines a grp/seq scope imposes: `Grp`
/// breaks its compositions all-or-nothing, `Seq` cascades a break forward.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    Grp,
    Seq,
}

/// A composition between two terms: its padding and how the enclosing grp/seq
/// scopes changed since the previous composition on the line. Scopes nest, so
/// the open scopes at any point form a stack: `closes` scopes pop off it and
/// `opens` (outermost first, a range into the document's shared scope buffer)
/// push onto it. `structure` replays these deltas to build the scope
/// graph.
///
/// Carrying deltas (total size O(number of scopes)) rather than each
/// composition's full enclosing scope stack (O(depth) per composition) is what
/// keeps the grp/seq passes linear on deeply nested scopes.
#[derive(Debug, Copy, Clone)]
pub(crate) struct FixedComp {
    pub(crate) pad: Pad,
    pub(crate) opens: Range<ScopeKind>,
    pub(crate) closes: u32,
}

/// One item of a line: a maximal run of terms joined by fixed compositions,
/// which never breaks. `terms` and `seps` are ranges into [`FixedDoc`]'s
/// shared `terms` and `run_seps` buffers; `seps[i]` sits between `terms[i]`
/// and `terms[i + 1]` (so `terms.len() == seps.len() + 1`).
#[derive(Debug, Copy, Clone)]
pub(crate) struct Run<'a> {
    pub(crate) terms: Range<Term<'a>>,
    pub(crate) seps: Range<FixedComp>,
}

/// One line: ranges into [`FixedDoc`]'s `items` and `item_seps` buffers.
/// `item_seps[seps.start + i]` is the breakable composition between the
/// line's item `i` and item `i + 1`.
#[derive(Debug, Copy, Clone)]
pub(crate) struct FixedLine<'a> {
    pub(crate) items: Range<Run<'a>>,
    pub(crate) seps: Range<FixedComp>,
}

/// The document as lines of runs. `lines` is the top-level index; the four
/// element buffers are shared across all lines (items and their separators)
/// and all runs (run terms and their separators), so building the document
/// appends instead of allocating per line or per run. The document also owns
/// the path arena its terms point into and the scope buffer its compositions'
/// deltas range into; it borrows only the layout's text.
#[derive(Debug)]
pub(crate) struct FixedDoc<'a> {
    pub(crate) lines: Vec<FixedLine<'a>>,
    pub(crate) items: Vec<Run<'a>>,
    pub(crate) item_seps: Vec<FixedComp>,
    pub(crate) terms: Vec<Term<'a>>,
    pub(crate) run_seps: Vec<FixedComp>,
    /// The shared nest/pack path arena every [`Term`]'s `path` points into.
    pub(crate) paths: Arena<PathNode>,
    /// The shared scope buffer every composition's `opens` ranges into.
    pub(crate) scopes: Vec<ScopeKind>,
}

/// A scope-chain accumulator: the innermost enclosing grp/seq wrapper, `None`
/// at the root.
type ChainId = Option<Id<ChainNode>>;

/// One grp/seq wrapper in the shared scope-chain arena. `parent` links to the
/// next-outer wrapper and `depth` is the chain length (root = 0), so two
/// chains — which share their outer spine by id — can be diffed by an
/// O(delta) walk: advance the deeper to equal depth, then step in lockstep to
/// the shared id.
#[derive(Copy, Clone)]
struct ChainNode {
    kind: ScopeKind,
    parent: ChainId,
    depth: u32,
}

/// How a leaf's term attaches to what follows it.
#[derive(Copy, Clone)]
enum Glue {
    /// A hard line break, or the end of the document.
    Line,
    /// A composition, under the captured scope chain.
    Comp { chain: ChainId, attr: Attr },
}

/// A pending subtree to visit, with its inherited context.
struct Work {
    node: LayId,
    /// Innermost nest/pack wrapper on the path so far.
    path: Option<PathId>,
    /// Innermost grp/seq wrapper on the path so far.
    chain: ChainId,
    /// How this subtree's last leaf attaches to what follows the subtree.
    glue: Glue,
    /// Under a `fix`: every composition is fixed.
    fixed: bool,
    /// Under a broken `seq`: every breakable composition is a line.
    broken: bool,
}

/// Accumulates the lines. Terms, run separators, items and item separators
/// are appended straight into the shared buffers; the line and run being
/// built are tracked as start offsets, so a line or run costs no allocation
/// of its own.
struct LineAccum<'a> {
    doc: FixedDoc<'a>,
    line_items_start: usize,
    line_seps_start: usize,
    run_terms_start: usize,
    run_seps_start: usize,
}

impl<'a> LineAccum<'a> {
    /// Extends the open run with `term` and the fixed composition `comp` that
    /// follows it.
    fn push_fixed(&mut self, term: Term<'a>, comp: FixedComp) {
        self.doc.terms.push(term);
        self.doc.run_seps.push(comp);
    }

    /// Ends the open run with `term` as its last term and appends it as the
    /// line's next item.
    fn push_item(&mut self, term: Term<'a>) {
        self.doc.terms.push(term);
        self.doc.items.push(Run {
            terms: Range::new(self.run_terms_start, self.doc.terms.len()),
            seps: Range::new(self.run_seps_start, self.doc.run_seps.len()),
        });
        self.run_terms_start = self.doc.terms.len();
        self.run_seps_start = self.doc.run_seps.len();
    }

    /// Ends the current line with `term` as its last term.
    fn flush_line(&mut self, term: Term<'a>) {
        self.push_item(term);
        self.doc.lines.push(FixedLine {
            items: Range::new(self.line_items_start, self.doc.items.len()),
            seps: Range::new(self.line_seps_start, self.doc.item_seps.len()),
        });
        self.line_items_start = self.doc.items.len();
        self.line_seps_start = self.doc.item_seps.len();
    }
}

/// `nodes` is a layout's postorder arena (root last) and `text` its text
/// buffer; the output borrows only `text`.
pub(crate) fn serialize<'a>(nodes: &Arena<LayoutNode>, text: &'a str) -> FixedDoc<'a> {
    let mut pack_count: u32 = 0;
    let mut chains: Arena<ChainNode> = Arena::new();
    let mut acc = LineAccum {
        doc: FixedDoc {
            lines: Vec::new(),
            items: Vec::new(),
            item_seps: Vec::new(),
            terms: Vec::new(),
            run_seps: Vec::new(),
            paths: Arena::new(),
            scopes: Vec::new(),
        },
        line_items_start: 0,
        line_seps_start: 0,
        run_terms_start: 0,
        run_seps_start: 0,
    };
    // Scratch for one composition's opens, reused across compositions.
    let mut opens: Vec<ScopeKind> = Vec::new();
    // The chain of the previous composition *on the same line*: grp/seq
    // scopes never cross a hard line, so it resets at every line.
    let mut prev: ChainId = None;

    // Pushing the right child before the left makes the left pop (and fully
    // process) first, so leaves are emitted in document order and the pack
    // counter advances in left-to-right pre-order.
    let mut stack: Vec<Work> = vec![Work {
        node: Id::from_index(nodes.len() - 1),
        path: None,
        chain: None,
        glue: Glue::Line,
        fixed: false,
        broken: false,
    }];
    while let Some(work) = stack.pop() {
        let Work {
            node,
            path,
            chain,
            glue,
            fixed,
            broken,
        } = work;
        match &nodes[node] {
            LayoutNode::Text(range) => {
                let term = Term {
                    path,
                    text: range.slice(text),
                };
                match glue {
                    Glue::Line => {
                        prev = None;
                        acc.flush_line(term);
                    }
                    Glue::Comp { chain, attr } => {
                        opens.clear();
                        let closes = diff_chains(&chains, prev, chain, &mut opens);
                        prev = chain;
                        // The diff walks the chain innermost-first; the delta
                        // lists opens outermost-first, in stack order.
                        opens.reverse();
                        let comp = FixedComp {
                            pad: attr.pad,
                            opens: append_range(&mut acc.doc.scopes, &opens),
                            closes,
                        };
                        if attr.brk == Break::Fixed {
                            acc.push_fixed(term, comp);
                        } else {
                            acc.push_item(term);
                            acc.doc.item_seps.push(comp);
                        }
                    }
                }
            }
            LayoutNode::Fix(child) => stack.push(Work {
                node: *child,
                path,
                chain,
                glue,
                fixed: true,
                broken: false,
            }),
            // A broken seq is dropped: its content is unconditionally broken.
            LayoutNode::Broken(child) => stack.push(Work {
                node: *child,
                path,
                chain,
                glue,
                fixed,
                broken: true,
            }),
            wrapper @ (LayoutNode::Grp(child) | LayoutNode::Seq(child)) => {
                let kind = match wrapper {
                    LayoutNode::Grp(_) => ScopeKind::Grp,
                    _ => ScopeKind::Seq,
                };
                let depth = chain.map_or(0, |id| chains[id].depth) + 1;
                let id = chains.push(ChainNode {
                    kind,
                    parent: chain,
                    depth,
                });
                stack.push(Work {
                    node: *child,
                    path,
                    chain: Some(id),
                    glue,
                    fixed,
                    broken: false,
                });
            }
            wrapper @ (LayoutNode::Nest(child) | LayoutNode::Pack(child)) => {
                let prop = match wrapper {
                    LayoutNode::Nest(_) => Prop::Nest,
                    _ => {
                        let index = pack_count;
                        pack_count += 1;
                        Prop::Pack(index)
                    }
                };
                let id = acc.doc.paths.push(PathNode { prop, parent: path });
                stack.push(Work {
                    node: *child,
                    path: Some(id),
                    chain,
                    glue,
                    fixed,
                    broken,
                });
            }
            LayoutNode::Line(left, right) => {
                stack.push(Work {
                    node: *right,
                    path,
                    chain,
                    glue,
                    fixed,
                    broken,
                });
                stack.push(Work {
                    node: *left,
                    path,
                    chain,
                    glue: Glue::Line,
                    fixed,
                    broken,
                });
            }
            LayoutNode::Comp(left, right, attr) => {
                // Under a broken seq every breakable composition is a hard
                // line (a fix inside a broken seq resets `broken`, so this
                // is decided on the composition's own attribute); every
                // composition that remains under a fix is fixed.
                let left_glue = if broken && attr.brk == Break::Breakable {
                    Glue::Line
                } else {
                    let brk = if fixed { Break::Fixed } else { attr.brk };
                    Glue::Comp {
                        chain,
                        attr: Attr { pad: attr.pad, brk },
                    }
                };
                stack.push(Work {
                    node: *right,
                    path,
                    chain,
                    glue,
                    fixed,
                    broken,
                });
                stack.push(Work {
                    node: *left,
                    path,
                    chain,
                    glue: left_glue,
                    fixed,
                    broken,
                });
            }
        }
    }

    acc.doc
}

/// Diffs two scope chains (innermost-first, sharing an outer spine by id):
/// the scopes that *open* (in `cur`, not `prev`) are appended innermost-first
/// to the caller's scratch, and the number that *close* (in `prev`, not
/// `cur`) is returned.
fn diff_chains(
    chains: &Arena<ChainNode>,
    prev: ChainId,
    cur: ChainId,
    opens: &mut Vec<ScopeKind>,
) -> u32 {
    let depth = |id: ChainId| id.map_or(0, |id| chains[id].depth);
    let mut a = prev; // contributes closes
    let mut b = cur; // contributes opens
    let (mut da, mut db) = (depth(a), depth(b));
    let mut closes = 0;
    // Drop the deeper chain's excess head down to the shallower chain's depth.
    while da > db {
        closes += 1;
        a = chains[a.expect("deeper chain is non-empty")].parent;
        da -= 1;
    }
    while db > da {
        let node = chains[b.expect("deeper chain is non-empty")];
        opens.push(node.kind);
        b = node.parent;
        db -= 1;
    }
    // Equal depth: step in lockstep until the shared spine — the first id both
    // chains agree on (or both `None`) — everything above it differs.
    while a != b {
        let nb = chains[b.expect("chains of equal depth")];
        closes += 1;
        opens.push(nb.kind);
        a = chains[a.expect("chains of equal depth")].parent;
        b = nb.parent;
    }
    closes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructors::{comp, fix, grp, line, nest, pack, seq, text};
    use crate::layout::Layout;

    const DEEP: usize = 50_000;

    fn run(layout: &Layout) -> FixedDoc<'_> {
        serialize(&layout.nodes, &layout.text)
    }

    /// The items of the single line of `doc`.
    fn one_line<'a>(doc: &'a FixedDoc<'a>) -> (&'a [Run<'a>], &'a [FixedComp]) {
        let [line] = &doc.lines[..] else {
            panic!("expected a single line")
        };
        (
            line.items.slice(&doc.items),
            line.seps.slice(&doc.item_seps),
        )
    }

    /// The texts of single-term runs.
    fn texts<'a>(doc: &FixedDoc<'a>, items: &[Run<'a>]) -> Vec<&'a str> {
        items
            .iter()
            .map(|run| match run.terms.slice(&doc.terms) {
                [term] => term.text,
                other => panic!("expected a single-term run, found {other:?}"),
            })
            .collect()
    }

    #[test]
    fn hard_lines_split_lines() {
        let layout = line(text("a"), line(text("b"), text("c")));
        let doc = run(&layout);
        assert_eq!(doc.lines.len(), 3);
    }

    #[test]
    fn broken_seq_turns_its_comps_into_lines_and_takes_no_scope() {
        // seq(a + (b @ c)): the seq contains a hard line, so its breakable
        // comp becomes a line and the seq claims no scope index.
        let layout = seq(comp(
            text("a"),
            line(text("b"), text("c")),
            Pad::Padded,
            Break::Breakable,
        ));
        let doc = run(&layout);
        assert_eq!(doc.lines.len(), 3);
        assert!(doc.scopes.is_empty());
        assert!(doc.item_seps.is_empty());
    }

    #[test]
    fn fixed_comp_survives_inside_broken_seq() {
        // seq((a !+ b) + (c @ d)): the fixed comp stays a two-term run on the
        // first line even though the enclosing seq is broken.
        let layout = seq(comp(
            comp(text("a"), text("b"), Pad::Padded, Break::Fixed),
            line(text("c"), text("d")),
            Pad::Padded,
            Break::Breakable,
        ));
        let doc = run(&layout);
        assert_eq!(doc.lines.len(), 3);
        let [first] = doc.lines[0].items.slice(&doc.items) else {
            panic!("expected the first line to be one run")
        };
        assert_eq!(first.terms.len(), 2);
    }

    #[test]
    fn broken_seq_under_fix_still_breaks_its_breakable_comps() {
        // fix(seq(a + (b @ c))): the seq is broken, so `+` becomes a line
        // even though the enclosing fix would otherwise have fixed it. Only
        // compositions that stay compositions are fixed.
        let layout = fix(seq(comp(
            text("a"),
            line(text("b"), text("c")),
            Pad::Padded,
            Break::Breakable,
        )));
        let doc = run(&layout);
        assert_eq!(doc.lines.len(), 3);
    }

    #[test]
    fn unbroken_seq_opens_a_scope_at_its_first_comp() {
        let layout = seq(comp(text("a"), text("b"), Pad::Padded, Break::Breakable));
        let doc = run(&layout);
        let (items, seps) = one_line(&doc);
        assert_eq!(texts(&doc, items), ["a", "b"]);
        let [sep] = seps else {
            panic!("expected one separator")
        };
        assert_eq!(sep.opens.slice(&doc.scopes), [ScopeKind::Seq]);
        assert_eq!(sep.closes, 0);
    }

    #[test]
    fn scope_closes_at_the_first_comp_outside_it() {
        // grp(a + b) + c: the grp opens at comp(a, b) and closes at comp(b, c).
        let layout = comp(
            grp(comp(text("a"), text("b"), Pad::Padded, Break::Breakable)),
            text("c"),
            Pad::Padded,
            Break::Breakable,
        );
        let doc = run(&layout);
        let (items, seps) = one_line(&doc);
        assert_eq!(texts(&doc, items), ["a", "b", "c"]);
        assert_eq!(seps.len(), 2);
        assert_eq!(seps[0].opens.len(), 1);
        assert_eq!(seps[1].closes, 1);
        assert_eq!(seps[1].opens.len(), 0);
    }

    #[test]
    fn opens_are_listed_outermost_first() {
        // grp(seq(a + b)): both open at comp(a, b), the grp outside the seq.
        let layout = grp(seq(comp(
            text("a"),
            text("b"),
            Pad::Padded,
            Break::Breakable,
        )));
        let doc = run(&layout);
        let (_, seps) = one_line(&doc);
        assert_eq!(
            seps[0].opens.slice(&doc.scopes),
            [ScopeKind::Grp, ScopeKind::Seq]
        );
    }

    #[test]
    fn fix_coalesces_everything_under_it() {
        // fix(a + (b & c)) & d: one run of three terms, then a run of one.
        let layout = comp(
            fix(comp(
                text("a"),
                comp(text("b"), text("c"), Pad::Unpadded, Break::Breakable),
                Pad::Padded,
                Break::Breakable,
            )),
            text("d"),
            Pad::Unpadded,
            Break::Breakable,
        );
        let doc = run(&layout);
        let (items, seps) = one_line(&doc);
        let [abc, d] = items else {
            panic!("expected two runs")
        };
        assert_eq!(abc.terms.len(), 3);
        assert_eq!(abc.seps.len(), 2);
        assert_eq!(d.terms.len(), 1);
        assert_eq!(seps.len(), 1);
    }

    #[test]
    fn nest_and_pack_build_shared_paths_with_preorder_pack_indices() {
        // pack(nest(a + b)): both leaves share one path (Nest under Pack 0).
        let layout = pack(nest(comp(
            text("a"),
            text("b"),
            Pad::Padded,
            Break::Breakable,
        )));
        let doc = run(&layout);
        let (items, _) = one_line(&doc);
        let paths: Vec<Option<PathId>> = items
            .iter()
            .map(|run| run.terms.slice(&doc.terms)[0].path)
            .collect();
        assert_eq!(paths[0], paths[1]);
        let inner = doc.paths[paths[0].expect("wrapped")];
        assert_eq!(inner.prop, Prop::Nest);
        let outer = doc.paths[inner.parent.expect("pack outside nest")];
        assert_eq!(outer.prop, Prop::Pack(0));
        assert!(outer.parent.is_none());
    }

    #[test]
    fn deep_right_nested_comp_chain() {
        let mut layout = text("z");
        for _ in 0..DEEP {
            layout = comp(text("y"), layout, Pad::Unpadded, Break::Breakable);
        }
        let doc = run(&layout);
        let (items, seps) = one_line(&doc);
        assert_eq!(items.len(), DEEP + 1);
        assert_eq!(seps.len(), DEEP);
    }

    #[test]
    fn deep_fixed_chain_is_one_run() {
        let mut layout = text("z");
        for _ in 0..DEEP {
            layout = comp(text("y"), layout, Pad::Unpadded, Break::Fixed);
        }
        let doc = run(&layout);
        let (items, _) = one_line(&doc);
        let [run] = items else {
            panic!("expected a single run")
        };
        assert_eq!(run.terms.len(), DEEP + 1);
    }

    #[test]
    fn deep_grp_and_nest_wrappers() {
        let mut layout = text("x");
        for _ in 0..DEEP {
            layout = nest(grp(layout));
        }
        let doc = run(&layout);
        let (items, _) = one_line(&doc);
        let [run] = items else {
            panic!("expected a single run")
        };
        let mut depth = 0;
        let mut cur = run.terms.slice(&doc.terms)[0].path;
        while let Some(id) = cur {
            depth += 1;
            cur = doc.paths[id].parent;
        }
        assert_eq!(depth, DEEP);
    }
}
