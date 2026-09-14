//! lines: Layout → lines of events
//!
//! One left-to-right DFS over the layout arena emits the document as a
//! stream of [`Event`]s: a text, a composition between two texts, and the
//! grp/seq scopes that open and close before a composition. [`Lines`]
//! lends the events one line at a time, since nothing downstream crosses a
//! hard line.
//!
//! - A hard line break ends a line. So does every breakable composition
//!   inside a *broken* sequence — a `seq` with a hard line beneath it, which
//!   the constructors mark as such — so its wrapper is dropped and its
//!   breakable compositions become lines. `fix` and `grp` reset that context.
//! - A composition is fixed or breakable; every composition under a `fix`
//!   is fixed. The texts a fixed composition joins form one unbreakable run,
//!   which the graph reads off the events.
//! - Each `nest`/`pack` descended through is a node of the shared path tree
//!   (a trie: one `Nest` child per node), so a text carries just its path
//!   and sibling leaves share their path spine.
//! - Each `grp`/`seq` descended through pushes one node onto a parent-linked
//!   scope-chain tree. Before each composition the line records how the
//!   chain changed since the previous composition on the line — the scopes
//!   that *close* and those that *open* — by diffing the two chains along
//!   their shared spine, which is O(delta), so deeply nested scopes stay
//!   linear. Every scope still open at the end of a line closes there, so
//!   every `Open` on a line has its `Close`.
//!
//! Pack indices are DFS pre-order counters, dense so the renderer keys its
//! marks by plain index.

use crate::arena::{Arena, Id, IdVec, Node, Tree};
use crate::layout::{Break, LayId, LayoutNode, Pad};

/// A nest/pack wrapper on a text. Pack indices are dense DFS counters.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum Indent {
    Nest,
    Pack(u32),
}

/// A node of the path tree: one nest/pack wrapper on the DFS path to a
/// leaf, under the next-outer wrapper.
pub(crate) type PathId = Id<Node<Indent>>;

/// The nest/pack wrappers on the paths to the leaves, as a trie: a node has
/// at most one `Nest` child, and a `Pack` node is unique to its index, so two
/// paths with the same wrappers outermost-in are the same node, and the
/// wrappers two texts share are exactly the chain of their lowest common
/// ancestor. Sibling leaves under the same wrappers share their spine, so
/// total path storage is O(input tree), not O(leaves × depth).
pub(crate) struct Paths {
    pub(crate) tree: Tree<Indent>,
    /// The `Nest` child of each node, once made.
    nest_child: IdVec<Node<Indent>, Option<PathId>>,
    /// The `Nest` child of the root.
    root_nest: Option<PathId>,
}

impl Paths {
    fn new() -> Self {
        Paths {
            tree: Tree::new(),
            nest_child: IdVec::new(),
            root_nest: None,
        }
    }

    /// The `Nest` node under `parent`, made on first use.
    fn nest(&mut self, parent: Option<PathId>) -> PathId {
        let slot = match parent {
            None => &mut self.root_nest,
            Some(parent) => &mut self.nest_child[parent],
        };
        if let Some(id) = *slot {
            return id;
        }
        let id = self.tree.push(Indent::Nest, parent);
        self.nest_child.push(None);
        match parent {
            None => self.root_nest = Some(id),
            Some(parent) => self.nest_child[parent] = Some(id),
        }
        id
    }

    /// A fresh `Pack` node under `parent`.
    fn pack(&mut self, parent: Option<PathId>, index: u32) -> PathId {
        let id = self.tree.push(Indent::Pack(index), parent);
        self.nest_child.push(None);
        id
    }
}

/// Which of the two breaking disciplines a grp/seq scope imposes: `Grp`
/// breaks its compositions all-or-nothing, `Seq` cascades a break forward.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum Scope {
    Grp,
    Seq,
}

/// One event of a line. Scopes nest, so the scopes open at any point form
/// a stack: `Close` pops it and `Open` pushes onto it; the opens before a
/// composition are listed outermost first. The graph replays the events.
///
/// Carrying deltas (total size O(number of scopes)) rather than each
/// composition's full enclosing scope stack (O(depth) per composition) is
/// what keeps the grp/seq passes linear on deeply nested scopes.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Event<'a> {
    /// A layout leaf: its innermost nest/pack wrapper (a node of the path
    /// tree, `None` for no wrappers) over its text. The empty layout is the
    /// empty text; both vanish in the emitter.
    Text { path: Option<PathId>, text: &'a str },
    /// A scope opens, before the next composition.
    Open(Scope),
    /// The innermost open scope closes, before the next composition or at
    /// the end of the line.
    Close,
    /// A composition between the text before it and the text after it.
    Comp(Pad, Break),
}

/// One line of the document, lent by [`Lines`]: its events, and the path
/// tree the texts' paths point into.
pub(crate) struct Line<'s, 'a> {
    pub(crate) events: &'s [Event<'a>],
    pub(crate) paths: &'s Paths,
}

/// The innermost enclosing grp/seq wrapper, a node of the scope-chain tree;
/// `None` at the root.
type ChainId = Option<Id<Node<Scope>>>;

/// How a leaf's text attaches to what follows it, as the DFS inherits it.
#[derive(Copy, Clone)]
enum Join {
    /// A hard line break, or the end of the document.
    Line,
    /// A composition, under the captured scope chain.
    Comp {
        chain: ChainId,
        pad: Pad,
        brk: Break,
    },
}

/// A pending subtree to visit, with its inherited context.
struct Work {
    node: LayId,
    /// Innermost nest/pack wrapper on the path so far.
    path: Option<PathId>,
    /// Innermost grp/seq wrapper on the path so far.
    chain: ChainId,
    /// How this subtree's last leaf attaches to what follows the subtree.
    join: Join,
    /// Under a `fix`: every composition is fixed.
    fixed: bool,
    /// Under a broken `seq`: every breakable composition is a line.
    broken: bool,
}

/// The DFS over a layout, paused between lines. `nodes` is the layout's
/// postorder arena (root last) and `text` its text buffer; the events
/// borrow only `text`.
pub(crate) struct Lines<'a> {
    nodes: &'a Arena<LayoutNode>,
    text: &'a str,
    /// Pending subtrees. Pushing the right child before the left makes the
    /// left pop (and fully process) first, so leaves are emitted in document
    /// order and the pack counter advances in left-to-right pre-order.
    stack: Vec<Work>,
    pack_count: u32,
    paths: Paths,
    chains: Tree<Scope>,
    /// The chain of the previous composition *on the same line*: grp/seq
    /// scopes never cross a hard line, so it resets at every line.
    prev: ChainId,
    /// The line being built, reused across lines.
    events: Vec<Event<'a>>,
}

impl<'a> Lines<'a> {
    pub(crate) fn new(nodes: &'a Arena<LayoutNode>, text: &'a str) -> Self {
        Lines {
            nodes,
            text,
            stack: vec![Work {
                node: Id::from_index(nodes.len() - 1),
                path: None,
                chain: None,
                join: Join::Line,
                fixed: false,
                broken: false,
            }],
            pack_count: 0,
            paths: Paths::new(),
            chains: Tree::new(),
            prev: None,
            events: Vec::new(),
        }
    }

    /// Runs the DFS up to the next hard line and lends that line; `None`
    /// once the layout is exhausted.
    pub(crate) fn next_line(&mut self) -> Option<Line<'_, 'a>> {
        self.events.clear();
        while let Some(work) = self.stack.pop() {
            let Work {
                node,
                path,
                chain,
                join,
                fixed,
                broken,
            } = work;
            match &self.nodes[node] {
                LayoutNode::Text(range) => {
                    let text = range.slice(self.text);
                    self.events.push(Event::Text { path, text });
                    match join {
                        Join::Line => {
                            // Every scope still open closes with the line.
                            let closes = self.chains.ancestors(self.prev, None).count();
                            self.events
                                .extend(std::iter::repeat_n(Event::Close, closes));
                            self.prev = None;
                            return Some(Line {
                                events: &self.events,
                                paths: &self.paths,
                            });
                        }
                        Join::Comp { chain, pad, brk } => {
                            self.delta(chain);
                            self.events.push(Event::Comp(pad, brk));
                        }
                    }
                }
                LayoutNode::Fix(child) => self.stack.push(Work {
                    node: *child,
                    path,
                    chain,
                    join,
                    fixed: true,
                    broken: false,
                }),
                // A broken seq is dropped: its content is unconditionally
                // broken.
                LayoutNode::Broken(child) => self.stack.push(Work {
                    node: *child,
                    path,
                    chain,
                    join,
                    fixed,
                    broken: true,
                }),
                wrapper @ (LayoutNode::Grp(child) | LayoutNode::Seq(child)) => {
                    let kind = match wrapper {
                        LayoutNode::Grp(_) => Scope::Grp,
                        _ => Scope::Seq,
                    };
                    let id = self.chains.push(kind, chain);
                    self.stack.push(Work {
                        node: *child,
                        path,
                        chain: Some(id),
                        join,
                        fixed,
                        broken: false,
                    });
                }
                wrapper @ (LayoutNode::Nest(child) | LayoutNode::Pack(child)) => {
                    let id = match wrapper {
                        LayoutNode::Nest(_) => self.paths.nest(path),
                        _ => {
                            let index = self.pack_count;
                            self.pack_count += 1;
                            self.paths.pack(path, index)
                        }
                    };
                    self.stack.push(Work {
                        node: *child,
                        path: Some(id),
                        chain,
                        join,
                        fixed,
                        broken,
                    });
                }
                LayoutNode::Line(left, right) => {
                    self.stack.push(Work {
                        node: *right,
                        path,
                        chain,
                        join,
                        fixed,
                        broken,
                    });
                    self.stack.push(Work {
                        node: *left,
                        path,
                        chain,
                        join: Join::Line,
                        fixed,
                        broken,
                    });
                }
                LayoutNode::Comp(left, right, pad, brk) => {
                    // Under a broken seq every breakable composition is a
                    // hard line (a fix inside a broken seq resets `broken`,
                    // so this is decided on the composition's own
                    // attribute); every composition that remains under a
                    // fix is fixed.
                    let left_join = if broken && *brk == Break::Breakable {
                        Join::Line
                    } else {
                        Join::Comp {
                            chain,
                            pad: *pad,
                            brk: if fixed { Break::Fixed } else { *brk },
                        }
                    };
                    self.stack.push(Work {
                        node: *right,
                        path,
                        chain,
                        join,
                        fixed,
                        broken,
                    });
                    self.stack.push(Work {
                        node: *left,
                        path,
                        chain,
                        join: left_join,
                        fixed,
                        broken,
                    });
                }
            }
        }
        None
    }

    /// Records how the scope chain changed since the previous composition
    /// on the line: the scopes beyond the shared spine of the two chains
    /// close (those in the previous chain only) and open (those in `chain`
    /// only, outermost first). The diff costs O(delta).
    fn delta(&mut self, chain: ChainId) {
        let chains = &self.chains;
        let shared = chains.lca(self.prev, chain);
        let closes = chains.ancestors(self.prev, shared).count();
        self.events
            .extend(std::iter::repeat_n(Event::Close, closes));
        let start = self.events.len();
        self.events.extend(
            chains
                .ancestors(chain, shared)
                .map(|id| Event::Open(chains[id].value)),
        );
        // The walk is innermost-first; opens are listed in stack order.
        self.events[start..].reverse();
        self.prev = chain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructors::{comp, fix, grp, line, nest, pack, seq, text};
    use crate::layout::Layout;

    const DEEP: usize = 50_000;

    /// A line's events, one token each: a text in quotes, `(grp`/`(seq`
    /// for an open, `)` for a close, and the DSL operator for a composition.
    fn show(events: &[Event<'_>]) -> String {
        events
            .iter()
            .map(|event| match *event {
                Event::Text { text, .. } => format!("{text:?}"),
                Event::Open(Scope::Grp) => "(grp".to_string(),
                Event::Open(Scope::Seq) => "(seq".to_string(),
                Event::Close => ")".to_string(),
                Event::Comp(pad, brk) => match (pad, brk) {
                    (Pad::Unpadded, Break::Breakable) => "&",
                    (Pad::Padded, Break::Breakable) => "+",
                    (Pad::Unpadded, Break::Fixed) => "!&",
                    (Pad::Padded, Break::Fixed) => "!+",
                }
                .to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Every line, shown.
    fn lines(layout: &Layout) -> Vec<String> {
        let mut lines = Lines::new(&layout.nodes, &layout.text);
        let mut shown = Vec::new();
        while let Some(line) = lines.next_line() {
            shown.push(show(line.events));
        }
        shown
    }

    /// The events of a one-line layout, copied out, with the path tree.
    fn one_line(layout: &Layout) -> (Vec<Event<'_>>, Paths) {
        let mut lines = Lines::new(&layout.nodes, &layout.text);
        let events = lines.next_line().expect("one line").events.to_vec();
        assert!(lines.next_line().is_none(), "expected a single line");
        (events, lines.paths)
    }

    fn texts<'a>(events: &[Event<'a>]) -> Vec<&'a str> {
        events
            .iter()
            .filter_map(|event| match *event {
                Event::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    fn paths(events: &[Event<'_>]) -> Vec<Option<PathId>> {
        events
            .iter()
            .filter_map(|event| match *event {
                Event::Text { path, .. } => Some(path),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn hard_lines_split_lines() {
        let layout = line(text("a"), line(text("b"), text("c")));
        assert_eq!(lines(&layout), [r#""a""#, r#""b""#, r#""c""#]);
    }

    #[test]
    fn broken_seq_turns_its_comps_into_lines_and_takes_no_scope() {
        // seq(a + (b @ c)): the seq contains a hard line, so its breakable
        // comp becomes a line and the seq claims no scope.
        let layout = seq(comp(
            text("a"),
            line(text("b"), text("c")),
            Pad::Padded,
            Break::Breakable,
        ));
        assert_eq!(lines(&layout), [r#""a""#, r#""b""#, r#""c""#]);
    }

    #[test]
    fn fixed_comp_survives_inside_broken_seq() {
        // seq((a !+ b) + (c @ d)): the fixed comp stays a two-text run on
        // the first line even though the enclosing seq is broken.
        let layout = seq(comp(
            comp(text("a"), text("b"), Pad::Padded, Break::Fixed),
            line(text("c"), text("d")),
            Pad::Padded,
            Break::Breakable,
        ));
        assert_eq!(lines(&layout), [r#""a" !+ "b""#, r#""c""#, r#""d""#]);
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
        assert_eq!(lines(&layout).len(), 3);
    }

    #[test]
    fn unbroken_seq_opens_at_its_first_comp_and_closes_with_the_line() {
        let layout = seq(comp(text("a"), text("b"), Pad::Padded, Break::Breakable));
        assert_eq!(lines(&layout), [r#""a" (seq + "b" )"#]);
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
        assert_eq!(lines(&layout), [r#""a" (grp + "b" ) + "c""#]);
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
        assert_eq!(lines(&layout), [r#""a" (grp (seq + "b" ) )"#]);
    }

    #[test]
    fn scopes_do_not_cross_a_hard_line() {
        // grp(a + (b @ c)) + d: the grp closes with the first line.
        let layout = comp(
            grp(comp(
                text("a"),
                line(text("b"), text("c")),
                Pad::Padded,
                Break::Breakable,
            )),
            text("d"),
            Pad::Padded,
            Break::Breakable,
        );
        assert_eq!(lines(&layout), [r#""a" (grp + "b" )"#, r#""c" + "d""#]);
    }

    #[test]
    fn fix_coalesces_everything_under_it() {
        // fix(a + (b & c)) & d: one run of three texts, then a run of one.
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
        assert_eq!(lines(&layout), [r#""a" !+ "b" !& "c" & "d""#]);
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
        let (events, paths_tree) = one_line(&layout);
        let [a, b] = paths(&events)[..] else {
            panic!("expected two texts")
        };
        assert_eq!(a, b);
        let inner = paths_tree.tree[a.expect("wrapped")];
        assert_eq!(inner.value, Indent::Nest);
        let outer = paths_tree.tree[inner.parent.expect("pack outside nest")];
        assert_eq!(outer.value, Indent::Pack(0));
        assert!(outer.parent.is_none());
    }

    #[test]
    fn paths_are_a_trie_with_one_nest_per_parent() {
        // nest(a) + nest(b): the two nests under the root are one node;
        // pack(c) + pack(d): every pack is its own node.
        let layout = comp(
            comp(
                nest(text("a")),
                nest(text("b")),
                Pad::Padded,
                Break::Breakable,
            ),
            comp(
                pack(text("c")),
                pack(text("d")),
                Pad::Padded,
                Break::Breakable,
            ),
            Pad::Padded,
            Break::Breakable,
        );
        let (events, _) = one_line(&layout);
        let [a, b, c, d] = paths(&events)[..] else {
            panic!("expected four texts")
        };
        assert_eq!(a, b);
        assert_ne!(c, d);
    }

    #[test]
    fn deep_right_nested_comp_chain() {
        let mut layout = text("z");
        for _ in 0..DEEP {
            layout = comp(text("y"), layout, Pad::Unpadded, Break::Breakable);
        }
        let (events, _) = one_line(&layout);
        assert_eq!(texts(&events).len(), DEEP + 1);
        assert_eq!(events.len(), 2 * DEEP + 1);
    }

    #[test]
    fn deep_fixed_chain_is_one_run() {
        let mut layout = text("z");
        for _ in 0..DEEP {
            layout = comp(text("y"), layout, Pad::Unpadded, Break::Fixed);
        }
        let (events, _) = one_line(&layout);
        assert!(
            events
                .iter()
                .all(|event| matches!(event, Event::Text { .. } | Event::Comp(_, Break::Fixed)))
        );
    }

    #[test]
    fn deep_grp_and_nest_wrappers() {
        let mut layout = text("x");
        for _ in 0..DEEP {
            layout = nest(grp(layout));
        }
        let (events, paths_tree) = one_line(&layout);
        let [path] = paths(&events)[..] else {
            panic!("expected a single text")
        };
        assert_eq!(paths_tree.tree.ancestors(path, None).count(), DEEP);
    }
}
