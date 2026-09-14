//! serialize: Layout → lines of terms with glue, scopes as deltas
//!
//! One left-to-right DFS over the layout arena emits the document's terms
//! in order, each with the *glue* that attaches it to the next: a hard line
//! or a composition. The [`Serializer`] lends the terms one line at a time,
//! since nothing downstream crosses a hard line.
//!
//! - A hard line break ends a line. So does every breakable composition
//!   inside a *broken* sequence — a `seq` with a hard line beneath it, which
//!   the constructors mark as such — so its wrapper is dropped and its
//!   breakable compositions become lines. `fix` and `grp` reset that context.
//! - A composition is fixed or breakable; every composition under a `fix`
//!   is fixed. The terms a fixed composition joins form one unbreakable run,
//!   which `structure` reads off the glue.
//! - Each `nest`/`pack` descended through is a node of the shared path tree
//!   (a trie: one `Nest` child per node), so a term is just (path, text)
//!   and sibling leaves share their path spine.
//! - Each `grp`/`seq` descended through pushes one node onto a parent-linked
//!   scope-chain tree. A composition records how the chain changed since the
//!   previous composition on its line — the scopes that *open* and how many
//!   *close* at it — by diffing the two chains along their shared spine,
//!   which is O(delta), so deeply nested scopes stay linear.
//!
//! Pack indices are DFS pre-order counters, dense so the renderer keys its
//! marks by plain index.

use crate::arena::{Arena, Id, IdVec, Node, Range, Tree, append_range};
use crate::layout::{Attr, Break, LayId, LayoutNode, Pad};

/// A nest/pack wrapper on a term. Pack indices are dense DFS counters.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum Prop {
    Nest,
    Pack(u32),
}

/// A node of the path tree: one nest/pack wrapper on the DFS path to a
/// leaf, under the next-outer wrapper.
pub(crate) type PathId = Id<Node<Prop>>;

/// The nest/pack wrappers on the paths to the leaves, as a trie: a node has
/// at most one `Nest` child, and a `Pack` node is unique to its index, so two
/// paths with the same wrappers outermost-in are the same node, and the
/// wrappers two terms share are exactly the chain of their lowest common
/// ancestor. Sibling leaves under the same wrappers share their spine, so
/// total path storage is O(input tree), not O(leaves × depth).
pub(crate) struct Paths {
    tree: Tree<Prop>,
    /// The `Nest` child of each node, once made.
    nest_child: IdVec<Node<Prop>, Option<PathId>>,
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
        let id = self.tree.push(Prop::Nest, parent);
        self.nest_child.push(None);
        match parent {
            None => self.root_nest = Some(id),
            Some(parent) => self.nest_child[parent] = Some(id),
        }
        id
    }

    /// A fresh `Pack` node under `parent`.
    fn pack(&mut self, parent: Option<PathId>, index: u32) -> PathId {
        let id = self.tree.push(Prop::Pack(index), parent);
        self.nest_child.push(None);
        id
    }
}

impl std::ops::Deref for Paths {
    type Target = Tree<Prop>;
    fn deref(&self) -> &Tree<Prop> {
        &self.tree
    }
}

/// Which of the two breaking disciplines a grp/seq scope imposes: `Grp`
/// breaks its compositions all-or-nothing, `Seq` cascades a break forward.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    Grp,
    Seq,
}

/// A composition between two terms: its attributes and how the enclosing
/// grp/seq scopes changed since the previous composition on the line. Scopes
/// nest, so the open scopes at any point form a stack: `closes` scopes pop
/// off it and `opens` (outermost first, a range into the line's scope
/// buffer) push onto it. `structure` replays these deltas to build the scope
/// graph.
///
/// Carrying deltas (total size O(number of scopes)) rather than each
/// composition's full enclosing scope stack (O(depth) per composition) is what
/// keeps the grp/seq passes linear on deeply nested scopes.
#[derive(Debug, Copy, Clone)]
pub(crate) struct Comp {
    pub(crate) pad: Pad,
    pub(crate) brk: Break,
    pub(crate) opens: Range<ScopeKind>,
    pub(crate) closes: u32,
}

/// How a term attaches to the next: the last term of a line ends it.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Glue {
    Line,
    Comp(Comp),
}

/// A layout leaf: its innermost nest/pack wrapper (a node of the path tree,
/// `None` for no wrappers) over its text, and its glue to the next term. The
/// empty layout is the empty text; both vanish in `structure`.
#[derive(Debug, Copy, Clone)]
pub(crate) struct Term<'a> {
    pub(crate) path: Option<PathId>,
    pub(crate) text: &'a str,
    pub(crate) next: Glue,
}

/// One line of the document, lent by the [`Serializer`]: its terms (the
/// last one's glue is [`Glue::Line`]), the path tree the terms' paths point
/// into, and the scope buffer the compositions' `opens` range into.
pub(crate) struct Line<'s, 'a> {
    pub(crate) terms: &'s [Term<'a>],
    pub(crate) paths: &'s Paths,
    pub(crate) scopes: &'s [ScopeKind],
}

/// The innermost enclosing grp/seq wrapper, a node of the scope-chain tree;
/// `None` at the root.
type ChainId = Option<Id<Node<ScopeKind>>>;

/// How a leaf's term attaches to what follows it, as the DFS inherits it.
#[derive(Copy, Clone)]
enum Join {
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
    join: Join,
    /// Under a `fix`: every composition is fixed.
    fixed: bool,
    /// Under a broken `seq`: every breakable composition is a line.
    broken: bool,
}

/// The DFS over a layout, paused between lines. `nodes` is the layout's
/// postorder arena (root last) and `text` its text buffer; the terms borrow
/// only `text`.
pub(crate) struct Serializer<'a> {
    nodes: &'a Arena<LayoutNode>,
    text: &'a str,
    /// Pending subtrees. Pushing the right child before the left makes the
    /// left pop (and fully process) first, so leaves are emitted in document
    /// order and the pack counter advances in left-to-right pre-order.
    stack: Vec<Work>,
    pack_count: u32,
    paths: Paths,
    chains: Tree<ScopeKind>,
    /// The chain of the previous composition *on the same line*: grp/seq
    /// scopes never cross a hard line, so it resets at every line.
    prev: ChainId,
    // The line being built, reused across lines.
    terms: Vec<Term<'a>>,
    scopes: Vec<ScopeKind>,
    /// Scratch for one composition's opens.
    opens: Vec<ScopeKind>,
}

impl<'a> Serializer<'a> {
    pub(crate) fn new(nodes: &'a Arena<LayoutNode>, text: &'a str) -> Self {
        Serializer {
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
            terms: Vec::new(),
            scopes: Vec::new(),
            opens: Vec::new(),
        }
    }

    /// Runs the DFS up to the next hard line and lends that line; `None`
    /// once the layout is exhausted.
    pub(crate) fn next_line(&mut self) -> Option<Line<'_, 'a>> {
        self.terms.clear();
        self.scopes.clear();
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
                    let next = match join {
                        Join::Line => {
                            self.prev = None;
                            Glue::Line
                        }
                        Join::Comp { chain, attr } => {
                            self.opens.clear();
                            let closes =
                                diff_chains(&self.chains, self.prev, chain, &mut self.opens);
                            self.prev = chain;
                            // The diff walks the chain innermost-first; the
                            // delta lists opens outermost-first, in stack
                            // order.
                            self.opens.reverse();
                            Glue::Comp(Comp {
                                pad: attr.pad,
                                brk: attr.brk,
                                opens: append_range(&mut self.scopes, &self.opens),
                                closes,
                            })
                        }
                    };
                    self.terms.push(Term { path, text, next });
                    if let Glue::Line = next {
                        return Some(Line {
                            terms: &self.terms,
                            paths: &self.paths,
                            scopes: &self.scopes,
                        });
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
                        LayoutNode::Grp(_) => ScopeKind::Grp,
                        _ => ScopeKind::Seq,
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
                LayoutNode::Comp(left, right, attr) => {
                    // Under a broken seq every breakable composition is a
                    // hard line (a fix inside a broken seq resets `broken`,
                    // so this is decided on the composition's own
                    // attribute); every composition that remains under a
                    // fix is fixed.
                    let left_join = if broken && attr.brk == Break::Breakable {
                        Join::Line
                    } else {
                        let brk = if fixed { Break::Fixed } else { attr.brk };
                        Join::Comp {
                            chain,
                            attr: Attr { pad: attr.pad, brk },
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
}

/// Diffs two scope chains: the scopes that *open* (in `cur`, not `prev`)
/// are appended innermost-first to the caller's scratch, and the number that
/// *close* (in `prev`, not `cur`) is returned. Both are the chains beyond
/// the shared spine, so the diff costs O(delta).
fn diff_chains(
    chains: &Tree<ScopeKind>,
    prev: ChainId,
    cur: ChainId,
    opens: &mut Vec<ScopeKind>,
) -> u32 {
    let shared = chains.lca(prev, cur);
    opens.extend(chains.ancestors(cur, shared).map(|id| chains[id].value));
    u32::try_from(chains.ancestors(prev, shared).count()).expect("closes fit u32")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructors::{comp, fix, grp, line, nest, pack, seq, text};
    use crate::layout::Layout;

    const DEEP: usize = 50_000;

    /// A line, copied out of the serializer.
    struct Owned<'a> {
        terms: Vec<Term<'a>>,
        scopes: Vec<ScopeKind>,
    }

    impl<'a> Owned<'a> {
        /// The items: maximal runs of terms joined by fixed compositions.
        fn items(&self) -> Vec<&[Term<'a>]> {
            let mut items = Vec::new();
            let mut start = 0;
            for (i, term) in self.terms.iter().enumerate() {
                if !matches!(
                    term.next,
                    Glue::Comp(Comp {
                        brk: Break::Fixed,
                        ..
                    })
                ) {
                    items.push(&self.terms[start..=i]);
                    start = i + 1;
                }
            }
            items
        }

        /// The breakable compositions between the items.
        fn seps(&self) -> Vec<Comp> {
            self.terms
                .iter()
                .filter_map(|term| match term.next {
                    Glue::Comp(
                        comp @ Comp {
                            brk: Break::Breakable,
                            ..
                        },
                    ) => Some(comp),
                    _ => None,
                })
                .collect()
        }

        /// The texts of the items, each a single term.
        fn texts(&self) -> Vec<&'a str> {
            self.items()
                .iter()
                .map(|item| match item {
                    [term] => term.text,
                    other => panic!("expected a single-term item, found {other:?}"),
                })
                .collect()
        }
    }

    fn lines(layout: &Layout) -> (Vec<Owned<'_>>, Paths) {
        let mut ser = Serializer::new(&layout.nodes, &layout.text);
        let mut lines = Vec::new();
        while let Some(line) = ser.next_line() {
            lines.push(Owned {
                terms: line.terms.to_vec(),
                scopes: line.scopes.to_vec(),
            });
        }
        (lines, ser.paths)
    }

    fn one_line(layout: &Layout) -> (Owned<'_>, Paths) {
        let (lines, paths) = lines(layout);
        let [line] = lines.try_into().ok().expect("expected a single line");
        (line, paths)
    }

    #[test]
    fn hard_lines_split_lines() {
        let layout = line(text("a"), line(text("b"), text("c")));
        let (lines, _) = lines(&layout);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| l.terms.len() == 1));
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
        let (lines, _) = lines(&layout);
        assert_eq!(lines.len(), 3);
        assert!(
            lines
                .iter()
                .all(|l| l.scopes.is_empty() && l.seps().is_empty())
        );
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
        let (lines, _) = lines(&layout);
        assert_eq!(lines.len(), 3);
        let [first] = lines[0].items()[..] else {
            panic!("expected the first line to be one run")
        };
        assert_eq!(first.len(), 2);
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
        let (lines, _) = lines(&layout);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn unbroken_seq_opens_a_scope_at_its_first_comp() {
        let layout = seq(comp(text("a"), text("b"), Pad::Padded, Break::Breakable));
        let (line, _) = one_line(&layout);
        assert_eq!(line.texts(), ["a", "b"]);
        let [sep] = line.seps()[..] else {
            panic!("expected one separator")
        };
        assert_eq!(sep.opens.slice(&line.scopes), [ScopeKind::Seq]);
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
        let (line, _) = one_line(&layout);
        assert_eq!(line.texts(), ["a", "b", "c"]);
        let seps = line.seps();
        assert_eq!(seps.len(), 2);
        assert_eq!(seps[0].opens.slice(&line.scopes), [ScopeKind::Grp]);
        assert_eq!(seps[1].closes, 1);
        assert_eq!(seps[1].opens.slice(&line.scopes), []);
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
        let (line, _) = one_line(&layout);
        assert_eq!(
            line.seps()[0].opens.slice(&line.scopes),
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
        let (line, _) = one_line(&layout);
        let [abc, d] = line.items()[..] else {
            panic!("expected two runs")
        };
        assert_eq!(abc.len(), 3);
        assert_eq!(d.len(), 1);
        assert_eq!(line.seps().len(), 1);
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
        let (line, paths) = one_line(&layout);
        let [a, b] = line.terms[..] else {
            panic!("expected two terms")
        };
        assert_eq!(a.path, b.path);
        let inner = paths[a.path.expect("wrapped")];
        assert_eq!(inner.value, Prop::Nest);
        let outer = paths[inner.parent.expect("pack outside nest")];
        assert_eq!(outer.value, Prop::Pack(0));
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
        let (line, _) = one_line(&layout);
        let [a, b, c, d] = line.terms[..] else {
            panic!("expected four terms")
        };
        assert_eq!(a.path, b.path);
        assert_ne!(c.path, d.path);
    }

    #[test]
    fn deep_right_nested_comp_chain() {
        let mut layout = text("z");
        for _ in 0..DEEP {
            layout = comp(text("y"), layout, Pad::Unpadded, Break::Breakable);
        }
        let (line, _) = one_line(&layout);
        assert_eq!(line.items().len(), DEEP + 1);
        assert_eq!(line.seps().len(), DEEP);
    }

    #[test]
    fn deep_fixed_chain_is_one_run() {
        let mut layout = text("z");
        for _ in 0..DEEP {
            layout = comp(text("y"), layout, Pad::Unpadded, Break::Fixed);
        }
        let (line, _) = one_line(&layout);
        let [run] = line.items()[..] else {
            panic!("expected a single run")
        };
        assert_eq!(run.len(), DEEP + 1);
    }

    #[test]
    fn deep_grp_and_nest_wrappers() {
        let mut layout = text("x");
        for _ in 0..DEEP {
            layout = nest(grp(layout));
        }
        let (line, paths) = one_line(&layout);
        let [term] = line.terms[..] else {
            panic!("expected a single term")
        };
        assert_eq!(paths.ancestors(term.path, None).count(), DEEP);
    }
}
