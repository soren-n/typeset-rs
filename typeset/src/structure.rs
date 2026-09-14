//! structure: lines of terms → Doc (solve the grp/seq scopes per line, then
//! emit)
//!
//! Scopes are ranges over a line's items, and items only exist once the
//! fixed compositions have been read as runs, so a scope's extent is not
//! readable off the layout tree. Per line, every item is a node and every
//! scope an edge from the node it opened at to the node it closed at: the
//! graph is the direct representation of those ranges, and the reference
//! implementation defines the widening rules over it.
//!
//! 1. [`Graph::build`] reads the items off the line's glue and replays each
//!    composition's scope deltas (the stack pushes and pops `serialize`
//!    recorded) into edges, in the order the scopes opened, which is
//!    document pre-order — the order `solve` and the emitter depend on.
//! 2. [`Graph::solve`] resolves nodes that both close and open scopes (a
//!    run that straddles a scope boundary, as in `grp(a + b) !& c`) by
//!    widening: leading seq edges out of the node are re-sourced onto the
//!    incoming side, and the incoming edges are handed forward past the
//!    first grp edge out.
//! 3. The emitter reads the line back into the [`Doc`], applying on the
//!    way the rules the reference runs as five tree rewrites afterwards:
//!    empty terms vanish, trivial grp/seq wrappers are dropped, every spine
//!    is right-nested, and shared nest/pack wrappers are factored out.
//!
//! Nothing crosses a hard line, so [`Structure`] consumes one line at a
//! time and every per-line structure — the graph, its edges, the spine
//! stacks — is scratch reused across lines. The graph's nodes are the
//! line's items (each a range of its terms) plus one edge arena; a node's
//! incident edges are intrusive linked lists threaded through the edge
//! arena, so `solve`'s surgery (pop a list head, insert before a known
//! edge, splice one list into another) is O(1) pointer rewiring and
//! building the graph allocates nothing per node or edge.

use crate::arena::{Arena, Id, IdVec, Range};
use crate::doc::{Doc, ObjId, ObjNode};
use crate::layout::{Break, Pad};
use crate::serialize::{Comp, Glue, Line, PathId, Paths, Prop, ScopeKind, Term};

/// The pass: consumes a serializer's lines one at a time into a [`Doc`].
pub(crate) struct Structure<'a> {
    graph: Graph<'a>,
    /// Which seq edges survive, from the counting walk.
    kept: IdVec<Edge<'a>, bool>,
    // Scratch, reused across lines.
    counting: Vec<Counting<'a>>,
    spines: Vec<Spine>,
    /// The elements of every open spine, each with the pad before it (the
    /// first element of a spine ignores its pad).
    elements: Vec<(Pad, Atom)>,
    doc: Doc,
}

impl<'a> Structure<'a> {
    /// `capacity` is a hint for the object arena: the layout's node count
    /// is about right.
    pub(crate) fn new(capacity: usize) -> Self {
        Structure {
            graph: Graph::new(),
            kept: IdVec::with_capacity(0),
            counting: Vec::new(),
            spines: Vec::new(),
            elements: Vec::new(),
            doc: Doc::with_capacity(capacity),
        }
    }

    /// Appends `line` to the document.
    pub(crate) fn push_line(&mut self, line: &Line<'_, 'a>) {
        self.graph.build(line);
        self.graph.solve();
        self.kept.reset(false, self.graph.edges.len());
        self.count_line(line);
        let root = self.emit_line(line);
        self.doc.lines.push(root);
    }

    pub(crate) fn finish(self) -> Doc {
        self.doc
    }
}

// --- The scope graph -------------------------------------------------------

/// A node is an item of the line: a run of terms joined by fixed
/// compositions, which never breaks.
type NodeId<'a> = Id<Node<'a>>;
type EdgeId<'a> = Id<Edge<'a>>;

/// An item and the ends of its intrusive edge lists.
#[derive(Debug, Copy, Clone)]
struct Node<'a> {
    /// The item's terms, a range of the line.
    terms: Range<Term<'a>>,
    /// Edges targeting this node, in list order (solve depends on the order).
    ins_head: Option<EdgeId<'a>>,
    ins_tail: Option<EdgeId<'a>>,
    /// Number of edges on the ins list.
    ins_len: u32,
    /// Edges sourced at this node, in list order (solve and the emitter
    /// depend on the order).
    outs_head: Option<EdgeId<'a>>,
    outs_tail: Option<EdgeId<'a>>,
}

impl<'a> Node<'a> {
    /// A node with no edges over `terms`.
    fn over(terms: Range<Term<'a>>) -> Self {
        Node {
            terms,
            ins_head: None,
            ins_tail: None,
            ins_len: 0,
            outs_head: None,
            outs_tail: None,
        }
    }
}

#[derive(Debug)]
struct Edge<'a> {
    kind: ScopeKind,
    source: NodeId<'a>,
    target: NodeId<'a>,
    /// Links within the source's outs list. `prev_out` exists because solve
    /// inserts before an arbitrary known edge of that list.
    next_out: Option<EdgeId<'a>>,
    prev_out: Option<EdgeId<'a>>,
    /// Link within the target's ins list. Ins lists are only appended to,
    /// iterated forward, spliced in *after* a known edge, or taken whole, so
    /// they need no back link.
    next_in: Option<EdgeId<'a>>,
}

/// A scope while its line is being built: where it opened, and where it
/// closed once it has.
#[derive(Copy, Clone)]
struct Slot<'a> {
    kind: ScopeKind,
    from: NodeId<'a>,
    to: Option<NodeId<'a>>,
}

/// The scope graph of one line; every buffer is reused across lines.
struct Graph<'a> {
    /// One node per item of the line, in order.
    nodes: Arena<Node<'a>>,
    /// The shared edge arena the intrusive lists thread through.
    edges: Arena<Edge<'a>>,
    // Build scratch: the scopes of the line in opening order, and the
    // indices of those still open.
    slots: Vec<Slot<'a>>,
    open: Vec<usize>,
}

impl<'a> Graph<'a> {
    fn new() -> Self {
        Graph {
            nodes: Arena::new(),
            edges: Arena::new(),
            slots: Vec::new(),
            open: Vec::new(),
        }
    }

    /// Reads the line's items off its glue — a breakable composition ends
    /// an item — and replays each composition's scope deltas at the node of
    /// the item before it. A run's internal comps and its trailing
    /// separator all share the run's node, exactly as document order
    /// threads them.
    ///
    /// The open scopes form a stack (a composition's closes pop from it, its
    /// opens push onto it), and scopes open in document pre-order, so
    /// materializing edges in opening order gives every node's ins and outs
    /// lists in pre-order.
    fn build(&mut self, line: &Line<'_, 'a>) {
        self.nodes.clear();
        self.edges.clear();
        self.slots.clear();
        self.open.clear();
        let mut start = 0;
        let mut node = self.nodes.push(Node::over(Range::new(0, 0)));
        for (i, term) in line.terms.iter().enumerate() {
            match term.next {
                Glue::Comp(comp) => {
                    self.delta(node, &comp, line.scopes);
                    if comp.brk == Break::Breakable {
                        self.nodes[node].terms = Range::new(start, i + 1);
                        start = i + 1;
                        node = self.nodes.push(Node::over(Range::new(start, start)));
                    }
                }
                Glue::Line => {
                    debug_assert_eq!(i + 1, line.terms.len(), "a line ends at its last term");
                    self.nodes[node].terms = Range::new(start, i + 1);
                }
            }
        }
        // Every scope still open closes at the line's last node.
        for &slot in &self.open {
            self.slots[slot].to = Some(node);
        }
        for i in 0..self.slots.len() {
            let slot = self.slots[i];
            let to = slot.to.expect("every scope closes by the end of its line");
            if slot.from != to {
                self.add_edge(slot.kind, slot.from, to);
            }
        }
    }

    /// Replays one composition's scope delta at `node`.
    fn delta(&mut self, node: NodeId<'a>, comp: &Comp, scopes: &[ScopeKind]) {
        for _ in 0..comp.closes {
            let slot = self.open.pop().expect("a closed scope was opened");
            self.slots[slot].to = Some(node);
        }
        for &kind in comp.opens.slice(scopes) {
            self.open.push(self.slots.len());
            self.slots.push(Slot {
                kind,
                from: node,
                to: None,
            });
        }
    }

    /// Appends an edge to its source's outs list and its target's ins list.
    fn add_edge(&mut self, kind: ScopeKind, source: NodeId<'a>, target: NodeId<'a>) {
        let edge = self.edges.push(Edge {
            kind,
            source,
            target,
            next_out: None,
            prev_out: None,
            next_in: None,
        });
        let src = &mut self.nodes[source];
        self.edges[edge].prev_out = src.outs_tail;
        match src.outs_tail {
            None => src.outs_head = Some(edge),
            Some(tail) => self.edges[tail].next_out = Some(edge),
        }
        self.nodes[source].outs_tail = Some(edge);
        let tgt = &mut self.nodes[target];
        match tgt.ins_tail {
            None => tgt.ins_head = Some(edge),
            Some(tail) => self.edges[tail].next_in = Some(edge),
        }
        self.nodes[target].ins_tail = Some(edge);
        self.nodes[target].ins_len += 1;
    }

    /// Resolves every node with both incoming and outgoing edges. Edges never
    /// cross lines, so one pass over the node table covers the document.
    fn solve(&mut self) {
        for node in self.nodes.ids() {
            self.solve_node(node);
        }
    }

    fn solve_node(&mut self, node: NodeId<'a>) {
        let (Some(ins_head), Some(_)) = (self.nodes[node].ins_head, self.nodes[node].outs_head)
        else {
            return;
        };

        // The incoming edge whose source is leftmost (first on ties: forward
        // iteration only replaces on strictly smaller sources).
        let mut ins_first = ins_head;
        let mut best_src = self.edges[ins_head].source;
        let mut e = self.edges[ins_head].next_in;
        while let Some(edge) = e {
            let src = self.edges[edge].source;
            if src < best_src {
                best_src = src;
                ins_first = edge;
            }
            e = self.edges[edge].next_in;
        }

        // Walk this node's outgoing edges, moving each leading seq edge out of
        // the way, until the first grp edge (or the end). A moved seq edge is
        // re-sourced onto the working edge's source, inserted immediately
        // before it, and becomes the working edge itself — so successive seq
        // edges stack up in front of `ins_first`.
        let mut edge = ins_first;
        let grp = loop {
            let Some(curr) = self.nodes[node].outs_head else {
                break None;
            };
            match self.edges[curr].kind {
                ScopeKind::Grp => break Some(curr),
                ScopeKind::Seq => {
                    self.pop_out_head(node);
                    let src = self.edges[edge].source;
                    self.insert_out_before(src, curr, edge);
                    self.edges[curr].source = src;
                    edge = curr;
                }
            }
        };

        // Hand this node's whole incoming list forward past the grp edge:
        // retarget every incoming edge to the grp's target and splice the list
        // immediately after the grp edge in that target's ins list.
        if let Some(grp) = grp {
            let head = self.nodes[node].ins_head;
            let tail = self.nodes[node].ins_tail.expect("ins list has a tail");
            let len = self.nodes[node].ins_len;
            self.nodes[node].ins_head = None;
            self.nodes[node].ins_tail = None;
            self.nodes[node].ins_len = 0;

            let target = self.edges[grp].target;
            let mut e = head;
            while let Some(edge) = e {
                self.edges[edge].target = target;
                e = self.edges[edge].next_in;
            }

            let after = self.edges[grp].next_in;
            self.edges[grp].next_in = head;
            self.edges[tail].next_in = after;
            if after.is_none() {
                self.nodes[target].ins_tail = Some(tail);
            }
            self.nodes[target].ins_len += len;
        }
    }

    /// Detaches the head edge of `node`'s outs list.
    fn pop_out_head(&mut self, node: NodeId<'a>) {
        let head = self.nodes[node].outs_head.expect("outs list is non-empty");
        let next = self.edges[head].next_out;
        self.nodes[node].outs_head = next;
        match next {
            None => self.nodes[node].outs_tail = None,
            Some(next) => self.edges[next].prev_out = None,
        }
        self.edges[head].next_out = None;
    }

    /// Inserts `new` immediately before `before` in `src`'s outs list.
    fn insert_out_before(&mut self, src: NodeId<'a>, new: EdgeId<'a>, before: EdgeId<'a>) {
        let prev = self.edges[before].prev_out;
        self.edges[new].prev_out = prev;
        self.edges[new].next_out = Some(before);
        self.edges[before].prev_out = Some(new);
        match prev {
            None => self.nodes[src].outs_head = Some(new),
            Some(prev) => self.edges[prev].next_out = Some(new),
        }
    }
}

// --- Emitting the document -------------------------------------------------
//
// After `solve`, a line's scopes nest: every node either closes scopes or
// opens them, never both, and the scopes open at any item form a stack. So
// a line reads back as a tree of *spines* — the line's own, and one per
// scope — each a left-to-right sequence of elements (an item, or a nested
// scope) with a pad between neighbours. The line is walked twice with a
// stack of open spines:
//
// - A counting walk decides which seqs survive. Empty items vanish; a scope
//   with no surviving element vanishes with them. A seq is kept when it
//   groups two or more compositions (a grp beneath it is opaque, a seq is
//   transparent) and is not directly under a seq.
// - The emitting walk lowers each run, threads the pad between surviving
//   neighbours (a vanished element's pads merge into the one composition
//   that remains; a spine's leading and trailing pads are dropped), decides
//   the grps (dropped when they group no composition, absorbed at the head
//   of their enclosing spine; seqs are transparent to the count), composes
//   each surviving spine right-nested with the nest/pack prefix its
//   operands share factored out, and splices a dropped scope's elements
//   into the spine that encloses it.
//
// The reference runs these as separate tree rewrites in exactly this order
// (null removal, seq identities, grp identities, reassociation, rescoping);
// the rules are not confluent, so the order is part of the semantics, and
// the two walks reproduce it: seq survival never depends on a grp
// decision, while grp absorption depends on which seqs survive.

/// How many compositions a spine contributes to its enclosing scope,
/// saturating at `Many`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Count {
    Zero,
    One,
    Many,
}

impl Count {
    fn add(self, other: Count) -> Count {
        match (self, other) {
            (Count::Zero, _) => other,
            (_, Count::Zero) => self,
            (Count::Many, _) | (_, Count::Many) | (Count::One, Count::One) => Count::Many,
        }
    }

    /// The compositions between `elements` neighbours.
    fn between(elements: usize) -> Count {
        match elements {
            0 | 1 => Count::Zero,
            2 => Count::One,
            _ => Count::Many,
        }
    }
}

/// A lowered element: the innermost nest/pack wrapper still to be applied
/// around its object (a node of the path tree, `None` for none).
type Atom = (Option<PathId>, ObjId);

/// A spine on the counting walk's stack.
struct Counting<'a> {
    /// The scope's edge; `None` for the line's own spine.
    edge: Option<EdgeId<'a>>,
    seq: bool,
    /// Surviving elements so far.
    elements: usize,
    /// Compositions contributed by transparent (seq) children.
    inner: Count,
}

/// What becomes of a spine when it closes.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Wrap {
    /// The line's own spine: its root object, if any.
    Line,
    /// A grp, unless it turns out to group no composition.
    Grp,
    /// A surviving seq.
    Seq,
    /// A dropped or absorbed scope: its elements join the enclosing spine.
    Splice,
}

/// A spine on the emitting walk's stack.
struct Spine {
    wrap: Wrap,
    /// Where this spine's elements begin in the shared element buffer.
    start: usize,
    /// Whether the spine's first surviving element is at the head of its
    /// enclosing group, which absorbs a grp there.
    head: bool,
    /// The pad to put before the next surviving element: `None` until the
    /// first survivor (leading pads are dropped), then the merge of every
    /// pad since the previous survivor.
    pending: Option<Pad>,
    /// Compositions contributed by surviving seq children, which are
    /// transparent to the grp count.
    inner: Count,
}

impl<'a> Structure<'a> {
    /// The counting walk: decides `kept` for every seq of the line.
    fn count_line(&mut self, line: &Line<'_, 'a>) {
        let g = &self.graph;
        self.counting.clear();
        self.counting.push(Counting {
            edge: None,
            seq: false,
            elements: 0,
            inner: Count::Zero,
        });
        for (_, node) in g.nodes.iter() {
            // The item is the first element of every scope opening here.
            let mut e = node.outs_head;
            while let Some(edge) = e {
                self.counting.push(Counting {
                    edge: Some(edge),
                    seq: g.edges[edge].kind == ScopeKind::Seq,
                    elements: 0,
                    inner: Count::Zero,
                });
                e = g.edges[edge].next_out;
            }
            // ... and the last element of every scope closing here,
            // innermost first; each closed scope is then an element of the
            // next.
            let mut survives = alive(line, node.terms);
            for _ in 0..node.ins_len {
                let mut top = self.counting.pop().expect("a closing scope is open");
                top.elements += usize::from(survives);
                let count = Count::between(top.elements).add(top.inner);
                let parent = self.counting.last_mut().expect("the line is never popped");
                if top.seq {
                    let edge = top.edge.expect("only scopes close");
                    self.kept[edge] = !parent.seq && count == Count::Many;
                    parent.inner = parent.inner.add(count);
                }
                survives = top.elements > 0;
            }
            let top = self.counting.last_mut().expect("the line is never popped");
            top.elements += usize::from(survives);
        }
    }

    /// The emitting walk: the line's root object, `None` if nothing
    /// survives.
    fn emit_line(&mut self, line: &Line<'_, 'a>) -> Option<ObjId> {
        self.spines.clear();
        self.elements.clear();
        self.spines.push(Spine {
            wrap: Wrap::Line,
            start: 0,
            head: true,
            pending: None,
            inner: Count::Zero,
        });
        for id in self.graph.nodes.ids() {
            let node = self.graph.nodes[id];
            assert!(
                node.ins_len == 0 || node.outs_head.is_none(),
                "solve leaves no node both closing and opening scopes"
            );
            let mut e = node.outs_head;
            while let Some(edge) = e {
                self.open(edge);
                e = self.graph.edges[edge].next_out;
            }
            let mut element = self.emit_run(line, node.terms);
            for _ in 0..node.ins_len {
                self.push(element);
                element = self.close(line.paths);
            }
            self.push(element);
            // The pad between this item and the next.
            if let Glue::Comp(sep) = line.terms[node.terms.end() - 1].next {
                let top = self.spines.last_mut().expect("the line is never popped");
                top.pending = top.pending.map(|pad| pad.merge(sep.pad));
            }
        }
        let top = self.spines.pop().expect("the line spine");
        assert!(
            self.spines.is_empty() && top.wrap == Wrap::Line,
            "every scope closes by the end of its line"
        );
        self.compose(line.paths, top.start)
            .map(|(path, obj)| wrap_path(&mut self.doc, line.paths, path, None, obj))
    }

    /// Opens the scope of `edge`: pushes its spine.
    fn open(&mut self, edge: EdgeId<'a>) {
        let parent = self.spines.last().expect("the line is never popped");
        // The scope is at the head of its enclosing group when it is the
        // first surviving element of a spine that is itself at the head.
        let head = parent.head && parent.pending.is_none();
        let (wrap, head) = match self.graph.edges[edge].kind {
            ScopeKind::Grp if head => (Wrap::Splice, true),
            ScopeKind::Grp => (Wrap::Grp, false),
            ScopeKind::Seq if self.kept[edge] => (Wrap::Seq, false),
            ScopeKind::Seq => (Wrap::Splice, head),
        };
        self.spines.push(Spine {
            wrap,
            start: self.elements.len(),
            head,
            pending: None,
            inner: Count::Zero,
        });
    }

    /// Appends a surviving element to the innermost spine, after the pad
    /// pending since the previous survivor.
    fn push(&mut self, element: Option<Atom>) {
        let Some(atom) = element else {
            return;
        };
        let top = self.spines.last_mut().expect("the line is never popped");
        self.elements
            .push((top.pending.unwrap_or(Pad::Unpadded), atom));
        top.pending = Some(Pad::Unpadded);
    }

    /// Closes the innermost scope, returning it as one element of the
    /// enclosing spine — or nothing, when it vanished or its elements were
    /// spliced into the enclosing spine instead.
    fn close(&mut self, paths: &Paths) -> Option<Atom> {
        let top = self.spines.pop().expect("a closing scope is open");
        let parent = self.spines.last_mut().expect("the line is never popped");
        let count = Count::between(self.elements.len() - top.start).add(top.inner);
        let wrap = match top.wrap {
            Wrap::Grp if count == Count::Zero => Wrap::Splice,
            wrap => wrap,
        };
        match wrap {
            Wrap::Line => unreachable!("the line spine closes with the line"),
            Wrap::Splice => {
                if let Some(first) = self.elements.get_mut(top.start) {
                    first.0 = parent.pending.unwrap_or(Pad::Unpadded);
                    parent.pending = Some(Pad::Unpadded);
                }
                None
            }
            Wrap::Grp | Wrap::Seq => {
                if wrap == Wrap::Seq {
                    parent.inner = parent.inner.add(count);
                }
                let (path, obj) = self.compose(paths, top.start)?;
                let obj = self.doc.push(match wrap {
                    Wrap::Grp => ObjNode::Grp(obj),
                    _ => ObjNode::Seq(obj),
                });
                Some((path, obj))
            }
        }
    }

    /// Composes the elements from `start` on as one right-nested spine,
    /// factoring at each composition the nest/pack wrappers its operands
    /// share — the chain of their paths' lowest common ancestor — and
    /// removes them. Returns the spine's remaining path and object, or
    /// nothing for an empty spine.
    fn compose(&mut self, paths: &Paths, start: usize) -> Option<Atom> {
        let elements = &self.elements[start..];
        let (mut res_path, mut result) = elements.last()?.1;
        // Innermost first, so each composition sees its right operand's
        // leftover wrappers.
        for k in (1..elements.len()).rev() {
            let (l_path, left) = elements[k - 1].1;
            let pad = elements[k].0;
            let shared = paths.lca(l_path, res_path);
            let left = wrap_path(&mut self.doc, paths, l_path, shared, left);
            let right = wrap_path(&mut self.doc, paths, res_path, shared, result);
            result = self.doc.push(ObjNode::Comp(left, right, pad));
            res_path = shared;
        }
        self.elements.truncate(start);
        Some((res_path, result))
    }

    /// Lowers a run: drops its empty terms, merges the pads between
    /// survivors (a dropped term's padding on either side folds into the one
    /// composition that remains), and keeps the first surviving term's path
    /// as the run's. `None` if nothing survived.
    fn emit_run(&mut self, line: &Line<'_, 'a>, terms: Range<Term<'a>>) -> Option<Atom> {
        let terms = terms.slice(line.terms);
        let mut first: Option<Option<PathId>> = None;
        // The pad to put before the next survivor: `None` until the first
        // survivor (its leading pads are dropped), then the merge of every
        // pad since the previous survivor.
        let mut pending: Option<Pad> = None;
        let start = self.doc.start_run();
        let last = terms.len() - 1;
        for (k, term) in terms.iter().enumerate() {
            if !term.text.is_empty() {
                self.doc
                    .push_text(pending.unwrap_or(Pad::Unpadded), term.text);
                first.get_or_insert(term.path);
                pending = Some(Pad::Unpadded);
            }
            // The glue after the last term is the item's, not the run's.
            if let (Some(p), true, Glue::Comp(sep)) = (pending.as_mut(), k < last, term.next) {
                *p = p.merge(sep.pad);
            }
        }
        Some((first?, self.doc.end_run(start)))
    }
}

/// Whether a run has any non-empty term.
fn alive<'a>(line: &Line<'_, 'a>, terms: Range<Term<'a>>) -> bool {
    terms
        .slice(line.terms)
        .iter()
        .any(|term| !term.text.is_empty())
}

/// Wraps `obj` in the wrappers from `path` outward up to (not including)
/// `upto`, innermost first, returning the outermost wrapper.
fn wrap_path(
    doc: &mut Doc,
    paths: &Paths,
    path: Option<PathId>,
    upto: Option<PathId>,
    obj: ObjId,
) -> ObjId {
    paths
        .ancestors(path, upto)
        .fold(obj, |obj, id| match paths[id].value {
            Prop::Nest => doc.push(ObjNode::Nest(obj)),
            Prop::Pack(index) => doc.push(ObjNode::Pack(index, obj)),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructors::{comp, fix, grp, nest, null, pack, seq, text};
    use crate::layout::{Break, Layout};

    /// The document of a one-line layout, printed as nested constructor
    /// names over the runs' texts.
    fn shape(layout: Layout) -> String {
        let doc = layout.compile();
        let [root] = doc.lines[..] else {
            panic!("expected one line")
        };
        fn obj(doc: &Doc, id: ObjId) -> String {
            match doc.objs[id] {
                ObjNode::Run(range) => range.slice(&doc.text).to_string(),
                ObjNode::Grp(c) => format!("Grp({})", obj(doc, c)),
                ObjNode::Seq(c) => format!("Seq({})", obj(doc, c)),
                ObjNode::Nest(c) => format!("Nest({})", obj(doc, c)),
                ObjNode::Pack(_, c) => format!("Pack({})", obj(doc, c)),
                ObjNode::Comp(l, r, _) => format!("Comp({}, {})", obj(doc, l), obj(doc, r)),
            }
        }
        root.map_or_else(|| "Empty".to_string(), |root| obj(&doc, root))
    }

    fn pad(l: Layout, r: Layout) -> Layout {
        comp(l, r, Pad::Padded, Break::Breakable)
    }

    fn fixed(l: Layout, r: Layout) -> Layout {
        comp(l, r, Pad::Padded, Break::Fixed)
    }

    #[test]
    fn nested_scopes_become_wrappers() {
        let layout = pad(text("x"), pad(grp(pad(text("a"), text("b"))), text("c")));
        assert_eq!(shape(layout), "Comp(x, Comp(Grp(Comp(a, b)), c))");
        let layout = seq(pad(
            text("a"),
            pad(text("b"), grp(pad(text("c"), text("d")))),
        ));
        assert_eq!(shape(layout), "Seq(Comp(a, Comp(b, Grp(Comp(c, d)))))");
    }

    #[test]
    fn scopes_opening_together_nest_in_opening_order() {
        // x + grp(seq(a + b + c)): both scopes open at a and close at c; the
        // grp, opened first, wraps the seq.
        let layout = pad(
            text("x"),
            grp(seq(pad(text("a"), pad(text("b"), text("c"))))),
        );
        assert_eq!(shape(layout), "Comp(x, Grp(Seq(Comp(a, Comp(b, c)))))");
    }

    #[test]
    fn scope_widens_to_cover_a_run_that_straddles_its_end() {
        // x + (grp(a + b) !+ c): the fixed composition coalesces b and c
        // into one run, so the grp cannot end between them; it widens to
        // include c.
        let layout = pad(text("x"), fixed(grp(pad(text("a"), text("b"))), text("c")));
        assert_eq!(shape(layout), "Comp(x, Grp(Comp(a, b c)))");
    }

    #[test]
    fn straddled_scopes_resolve_seq_outward_and_grp_inward() {
        // seq(a + b + c) !+ grp(d + e): the run [c d] both closes the seq
        // and opens the grp. The seq's end is handed past the grp, so the
        // seq covers everything and the grp sits inside it.
        let layout = fixed(
            seq(pad(text("a"), pad(text("b"), text("c")))),
            grp(pad(text("d"), text("e"))),
        );
        assert_eq!(shape(layout), "Seq(Comp(a, Comp(b, Grp(Comp(c d, e)))))");
    }

    #[test]
    fn a_fix_is_one_run() {
        let layout = fix(pad(pad(text("a"), text("b")), text("c")));
        assert_eq!(shape(layout), "a b c");
    }

    #[test]
    fn spines_are_right_nested_across_dropped_scopes() {
        // (a + b) + grp(c) + d: the grp groups nothing and is dropped; the
        // whole line is one right-nested spine.
        let layout = pad(pad(text("a"), text("b")), pad(grp(text("c")), text("d")));
        assert_eq!(shape(layout), "Comp(a, Comp(b, Comp(c, d)))");
    }

    #[test]
    fn a_grp_at_the_head_of_its_group_is_absorbed() {
        assert_eq!(
            shape(pad(grp(pad(text("a"), text("b"))), text("c"))),
            "Comp(a, Comp(b, c))"
        );
        // Inside a kept seq the head resets, so the grp survives.
        assert_eq!(
            shape(seq(pad(
                grp(pad(text("a"), text("b"))),
                pad(text("c"), text("d"))
            ))),
            "Seq(Comp(Grp(Comp(a, b)), Comp(c, d)))"
        );
    }

    #[test]
    fn a_seq_needs_two_compositions_and_no_seq_above_it() {
        assert_eq!(shape(seq(pad(text("a"), text("b")))), "Comp(a, b)");
        assert_eq!(
            shape(seq(pad(text("a"), seq(pad(text("b"), text("c")))))),
            "Seq(Comp(a, Comp(b, c)))"
        );
        // A grp beneath a seq is opaque to its count.
        assert_eq!(
            shape(pad(
                text("x"),
                seq(grp(pad(text("a"), pad(text("b"), text("c")))))
            )),
            "Comp(x, Grp(Comp(a, Comp(b, c))))"
        );
    }

    #[test]
    fn empty_texts_vanish_and_their_pads_merge() {
        assert_eq!(shape(null()), "Empty");
        assert_eq!(shape(pad(nest(null()), text("a"))), "a");
        // The pads around a vanished middle element merge; a vanished
        // leading element's pad is dropped even across a wrapper.
        let layout = comp(
            text("a"),
            comp(null(), text("b"), Pad::Padded, Break::Breakable),
            Pad::Unpadded,
            Break::Breakable,
        );
        let doc = layout.compile();
        let root = doc.lines[0].expect("a survives");
        assert!(matches!(doc.objs[root], ObjNode::Comp(_, _, Pad::Padded)));
        let layout = comp(
            text("a"),
            grp(comp(null(), text("b"), Pad::Padded, Break::Breakable)),
            Pad::Unpadded,
            Break::Breakable,
        );
        let doc = layout.compile();
        let root = doc.lines[0].expect("a survives");
        assert!(matches!(doc.objs[root], ObjNode::Comp(_, _, Pad::Unpadded)));
    }

    #[test]
    fn shared_wrapper_prefixes_are_factored_out() {
        let layout = pack(nest(pad(text("a"), text("b"))));
        assert_eq!(shape(layout), "Pack(Nest(Comp(a, b)))");
        let layout = pad(nest(text("a")), nest(text("b")));
        assert_eq!(shape(layout), "Nest(Comp(a, b))");
        let layout = pad(nest(text("a")), text("b"));
        assert_eq!(shape(layout), "Comp(Nest(a), b)");
    }
}
