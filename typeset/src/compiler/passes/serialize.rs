//! serialize: Layout → FixedDoc (lines of items, scopes as deltas)
//!
//! One left-to-right DFS over the layout arena emits the document as lines of
//! items with the compositions between them:
//!
//! - A hard line break ends a line. So does every breakable composition
//!   inside a *broken* sequence — a `seq` whose subtree contains a hard line
//!   is unconditionally broken, so its wrapper is dropped and its breakable
//!   compositions become lines. `fix` and `grp` reset that context.
//! - Maximal runs of terms joined by fixed compositions (including every
//!   composition under a `fix`) coalesce into single fix items.
//! - Each `nest`/`pack` descended through pushes one node onto the shared
//!   path arena, so a term is just (path id, leaf) and sibling leaves share
//!   their path spine.
//! - Each `grp`/`seq` descended through pushes one node onto a parent-linked
//!   scope-chain arena (assigning the scope's index in DFS pre-order). A
//!   composition records how the chain changed since the previous composition
//!   on its line — the scopes that *open* and *close* at it — by diffing the
//!   two chains along their shared spine, which is O(delta), so deeply nested
//!   scopes stay linear.
//!
//! Scope and pack indices are DFS pre-order counters; `resolve_scopes` keys
//! the scope graph by scope index.

use crate::compiler::types::{
    Arena, Attr, Break, Id, IdVec, LayId, LayoutNode, Pad, PathId, PathNode, Prop, Range, Scope,
    ScopeKind, Term, TermLeaf, append_range,
};

/// A composition between two items: its padding and the scopes opening and
/// closing here (ranges into the document's shared scope buffer).
#[derive(Debug, Copy, Clone)]
pub(crate) struct FixedComp {
    pub(crate) pad: Pad,
    pub(crate) opens: Range<Scope>,
    pub(crate) closes: Range<Scope>,
}

/// A maximal run of terms joined by fixed compositions, coalesced into one
/// unbreakable item. `terms` and `seps` are ranges into [`FixedDoc`]'s shared
/// `terms` and `run_seps` buffers; `seps[i]` sits between `terms[i]` and
/// `terms[i + 1]` (so `terms.len() == seps.len() + 1`).
#[derive(Debug, Copy, Clone)]
pub(crate) struct FixRun<'a> {
    pub(crate) terms: Range<Term<'a>>,
    pub(crate) seps: Range<FixedComp>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum FixedItem<'a> {
    Term(Term<'a>),
    Fix(FixRun<'a>),
}

/// One line: ranges into [`FixedDoc`]'s `items` and `item_seps` buffers.
/// `item_seps[seps.start + i]` is the non-fixed composition between the line's
/// item `i` and item `i + 1`.
#[derive(Debug, Copy, Clone)]
pub(crate) struct FixedLine<'a> {
    pub(crate) items: Range<FixedItem<'a>>,
    pub(crate) seps: Range<FixedComp>,
}

/// The document as lines of items. `lines` is the top-level index; the four
/// element buffers are shared across all lines (items and their separators)
/// and all fix runs (run terms and their separators), so building the
/// document appends instead of allocating per line or per run. The document
/// also owns the path arena its terms point into and the scope buffer its
/// compositions' deltas range into; it borrows only the layout's text.
#[derive(Debug)]
pub(crate) struct FixedDoc<'a> {
    pub(crate) lines: Vec<FixedLine<'a>>,
    pub(crate) items: Vec<FixedItem<'a>>,
    pub(crate) item_seps: Vec<FixedComp>,
    pub(crate) terms: Vec<Term<'a>>,
    pub(crate) run_seps: Vec<FixedComp>,
    /// The shared nest/pack path arena every [`Term`]'s `path` points into.
    pub(crate) paths: Arena<PathNode>,
    /// The shared scope buffer every delta ranges into.
    pub(crate) scopes: Vec<Scope>,
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
    scope: Scope,
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

/// Accumulates the lines. Items, line separators, run terms and run
/// separators are appended straight into the shared buffers; the line and fix
/// run being built are tracked as start offsets, so a line or run costs no
/// allocation of its own.
struct LineAccum<'a> {
    doc: FixedDoc<'a>,
    line_items_start: usize,
    line_seps_start: usize,
    /// Start offsets of the fix run being coalesced, if one is.
    run_start: Option<(usize, usize)>,
}

impl<'a> LineAccum<'a> {
    /// Extends (or starts) the open fix run with `term` and the fixed
    /// composition `comp` that follows it.
    fn push_fixed(&mut self, term: Term<'a>, comp: FixedComp) {
        if self.run_start.is_none() {
            self.run_start = Some((self.doc.terms.len(), self.doc.run_seps.len()));
        }
        self.doc.terms.push(term);
        self.doc.run_seps.push(comp);
    }

    /// Appends `term` as the line's next item: as the final term of the open
    /// fix run if one is being built, else as a plain term.
    fn push_item(&mut self, term: Term<'a>) {
        let Some((terms_start, seps_start)) = self.run_start.take() else {
            self.doc.items.push(FixedItem::Term(term));
            return;
        };
        self.doc.terms.push(term);
        self.doc.items.push(FixedItem::Fix(FixRun {
            terms: Range::new(terms_start, self.doc.terms.len()),
            seps: Range::new(seps_start, self.doc.run_seps.len()),
        }));
    }

    /// Ends the current line with `term` as its last item.
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
pub fn serialize<'a>(nodes: &Arena<LayoutNode>, text: &'a str) -> FixedDoc<'a> {
    // Whether each subtree contains a hard line break: a bottom-up fold, so a
    // `seq` can be classified as broken when the DFS reaches it.
    let mut has_line: IdVec<LayoutNode, bool> = IdVec::with_capacity(nodes.len());
    for (_, node) in nodes.iter() {
        let flag = match *node {
            LayoutNode::Null | LayoutNode::Text(_) => false,
            LayoutNode::Fix(c)
            | LayoutNode::Grp(c)
            | LayoutNode::Seq(c)
            | LayoutNode::Nest(c)
            | LayoutNode::Pack(c) => has_line[c],
            LayoutNode::Line(..) => true,
            LayoutNode::Comp(l, r, _) => has_line[l] || has_line[r],
        };
        has_line.push(flag);
    }

    let mut scope_count: u32 = 0;
    let mut pack_count: u32 = 0;
    let mut paths: Arena<PathNode> = Arena::new();
    let mut chains: Arena<ChainNode> = Arena::new();
    // Each leaf, in document order, with its glue.
    let mut leaves: Vec<(Glue, Term<'a>)> = Vec::new();

    // Pushing the right child before the left makes the left pop (and fully
    // process) first, so the counters advance in left-to-right pre-order.
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
            leaf @ (LayoutNode::Null | LayoutNode::Text(_)) => {
                let leaf = match leaf {
                    LayoutNode::Text(range) => TermLeaf::Text(range.slice(text)),
                    _ => TermLeaf::Null,
                };
                leaves.push((glue, Term { path, leaf }));
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
            LayoutNode::Seq(child) if has_line[*child] => stack.push(Work {
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
                let scope = Scope {
                    kind,
                    index: scope_count,
                };
                scope_count += 1;
                let depth = chain.map_or(0, |id| chains[id].depth) + 1;
                let id = chains.push(ChainNode {
                    scope,
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
                let id = paths.push(PathNode { prop, parent: path });
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

    // Lay the leaves out as lines, resolving each composition's scope deltas
    // against the previous composition's chain *on the same line*: grp/seq
    // scopes never cross a hard line, so the chain resets at every line.
    let mut acc = LineAccum {
        doc: FixedDoc {
            lines: Vec::new(),
            items: Vec::new(),
            item_seps: Vec::new(),
            terms: Vec::new(),
            run_seps: Vec::new(),
            paths,
            scopes: Vec::new(),
        },
        line_items_start: 0,
        line_seps_start: 0,
        run_start: None,
    };
    // Scratch for one composition's deltas, reused across compositions.
    let mut opens: Vec<Scope> = Vec::new();
    let mut closes: Vec<Scope> = Vec::new();
    let mut prev: ChainId = None;
    for &(glue, term) in &leaves {
        match glue {
            Glue::Line => {
                prev = None;
                acc.flush_line(term);
            }
            Glue::Comp { chain, attr } => {
                opens.clear();
                closes.clear();
                diff_chains(&chains, prev, chain, &mut opens, &mut closes);
                prev = chain;
                let comp = FixedComp {
                    pad: attr.pad,
                    opens: append_range(&mut acc.doc.scopes, &opens),
                    closes: append_range(&mut acc.doc.scopes, &closes),
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
    acc.doc
}

/// Diffs two scope chains (innermost-first, sharing an outer spine by id)
/// into the scopes that *open* (in `cur`, not `prev`) and *close* (in `prev`,
/// not `cur`), appended to the caller's scratch. Order within each delta is
/// irrelevant: `resolve_scopes` keys scopes by index.
fn diff_chains(
    chains: &Arena<ChainNode>,
    prev: ChainId,
    cur: ChainId,
    opens: &mut Vec<Scope>,
    closes: &mut Vec<Scope>,
) {
    let depth = |id: ChainId| id.map_or(0, |id| chains[id].depth);
    let mut a = prev; // contributes closes
    let mut b = cur; // contributes opens
    let (mut da, mut db) = (depth(a), depth(b));
    // Drop the deeper chain's excess head down to the shallower chain's depth.
    while da > db {
        let node = chains[a.expect("deeper chain is non-empty")];
        closes.push(node.scope);
        a = node.parent;
        da -= 1;
    }
    while db > da {
        let node = chains[b.expect("deeper chain is non-empty")];
        opens.push(node.scope);
        b = node.parent;
        db -= 1;
    }
    // Equal depth: step in lockstep until the shared spine — the first id both
    // chains agree on (or both `None`) — everything above it differs.
    while a != b {
        let na = chains[a.expect("chains of equal depth")];
        let nb = chains[b.expect("chains of equal depth")];
        closes.push(na.scope);
        opens.push(nb.scope);
        a = na.parent;
        b = nb.parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::constructors::{comp, fix, grp, line, nest, pack, seq, text};
    use crate::compiler::types::Layout;

    /// Deeper than a native-stack recursion could survive.
    const DEEP: usize = 50_000;

    fn run(layout: &Layout) -> FixedDoc<'_> {
        serialize(&layout.nodes, &layout.text)
    }

    /// The items of the single line of `doc`.
    fn one_line<'a>(doc: &'a FixedDoc<'a>) -> (&'a [FixedItem<'a>], &'a [FixedComp]) {
        let [line] = &doc.lines[..] else {
            panic!("expected a single line")
        };
        (
            line.items.slice(&doc.items),
            line.seps.slice(&doc.item_seps),
        )
    }

    fn texts<'a>(items: &[FixedItem<'a>]) -> Vec<&'a str> {
        items
            .iter()
            .map(|item| match item {
                FixedItem::Term(Term {
                    leaf: TermLeaf::Text(t),
                    ..
                }) => *t,
                other => panic!("expected a text term, found {other:?}"),
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
    }

    #[test]
    fn fixed_comp_survives_inside_broken_seq() {
        // seq((a !+ b) + (c @ d)): the fixed comp stays a fix run on the
        // first line even though the enclosing seq is broken.
        let layout = seq(comp(
            comp(text("a"), text("b"), Pad::Padded, Break::Fixed),
            line(text("c"), text("d")),
            Pad::Padded,
            Break::Breakable,
        ));
        let doc = run(&layout);
        assert_eq!(doc.lines.len(), 3);
        let first = doc.lines[0].items.slice(&doc.items);
        let [FixedItem::Fix(run)] = first else {
            panic!("expected the first line to be one fix run")
        };
        assert_eq!(run.terms.len(), 2);
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
        assert_eq!(texts(items), ["a", "b"]);
        let [sep] = seps else {
            panic!("expected one separator")
        };
        let opens = sep.opens.slice(&doc.scopes);
        assert!(matches!(
            opens,
            [Scope {
                kind: ScopeKind::Seq,
                index: 0
            }]
        ));
        assert!(sep.closes.slice(&doc.scopes).is_empty());
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
        assert_eq!(texts(items), ["a", "b", "c"]);
        assert_eq!(seps.len(), 2);
        assert_eq!(seps[0].opens.len(), 1);
        assert_eq!(seps[1].closes.len(), 1);
        assert_eq!(seps[1].opens.len(), 0);
    }

    #[test]
    fn fix_coalesces_everything_under_it() {
        // fix(a + (b & c)) & d: one fix run of three terms, then a plain term.
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
        let [FixedItem::Fix(run), FixedItem::Term(_)] = items else {
            panic!("expected a fix run then a plain term")
        };
        assert_eq!(run.terms.len(), 3);
        assert_eq!(run.seps.len(), 2);
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
            .map(|item| match item {
                FixedItem::Term(term) => term.path,
                _ => panic!("expected terms"),
            })
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
        let [FixedItem::Fix(run)] = items else {
            panic!("expected a single fix run")
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
        let [FixedItem::Term(term)] = items else {
            panic!("expected a single term")
        };
        let mut depth = 0;
        let mut cur = term.path;
        while let Some(id) = cur {
            depth += 1;
            cur = doc.paths[id].parent;
        }
        assert_eq!(depth, DEEP);
    }
}
