//! graph: the grp/seq scopes of one line, solved
//!
//! Scopes are ranges over a line's items, and items only exist once the
//! fixed compositions have been read as runs, so a scope's extent is not
//! readable off the layout tree. Per line, every item is a node and every
//! scope an edge from the node it opened at to the node it closed at: the
//! graph is the direct representation of those ranges, and the reference
//! implementation defines the widening rules over it.
//!
//! 1. [`Graph::build`] reads the items off the line's events — a breakable
//!    composition ends an item — and replays the scope opens and closes
//!    into edges, in the order the scopes opened, which is document
//!    pre-order — the order `solve` and the emitter depend on.
//! 2. [`Graph::solve`] resolves nodes that both close and open scopes (a
//!    run that straddles a scope boundary, as in `grp(a + b) !& c`) by
//!    widening: leading seq edges out of the node are re-sourced onto the
//!    incoming side, and the incoming edges are handed forward past the
//!    first grp edge out.
//!
//! Nothing crosses a hard line, so every buffer is scratch reused across
//! lines. The graph's nodes are the line's items (each a range of its
//! events) plus one edge arena; a node's incident edges are intrusive
//! linked lists threaded through the edge arena, so `solve`'s surgery (pop
//! a list head, insert before a known edge, splice one list into another)
//! is O(1) pointer rewiring and building the graph allocates nothing per
//! node or edge.

use crate::arena::{Arena, Id, Range};
use crate::layout::Break;
use crate::lines::{Event, Line, Scope};

/// A node is an item of the line: a run of texts joined by fixed
/// compositions, which never breaks.
pub(crate) type NodeId<'a> = Id<Node<'a>>;
pub(crate) type EdgeId<'a> = Id<Edge<'a>>;

/// An item and the ends of its intrusive edge lists.
#[derive(Debug, Copy, Clone)]
pub(crate) struct Node<'a> {
    /// The item's events, a range of the line: its texts and the fixed
    /// compositions and scope opens and closes among them. The breakable
    /// composition after the item, if any, is the event just past the range.
    pub(crate) events: Range<Event<'a>>,
    /// Edges targeting this node, in list order (solve depends on the order).
    pub(crate) ins_head: Option<EdgeId<'a>>,
    ins_tail: Option<EdgeId<'a>>,
    /// Number of edges on the ins list.
    pub(crate) ins_len: u32,
    /// Edges sourced at this node, in list order (solve and the emitter
    /// depend on the order).
    pub(crate) outs_head: Option<EdgeId<'a>>,
    outs_tail: Option<EdgeId<'a>>,
}

impl<'a> Node<'a> {
    /// A node with no edges over `events`.
    fn over(events: Range<Event<'a>>) -> Self {
        Node {
            events,
            ins_head: None,
            ins_tail: None,
            ins_len: 0,
            outs_head: None,
            outs_tail: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Edge<'a> {
    pub(crate) kind: Scope,
    source: NodeId<'a>,
    target: NodeId<'a>,
    /// Links within the source's outs list. `prev_out` exists because solve
    /// inserts before an arbitrary known edge of that list.
    pub(crate) next_out: Option<EdgeId<'a>>,
    prev_out: Option<EdgeId<'a>>,
    /// Link within the target's ins list. Ins lists are only appended to,
    /// iterated forward, spliced in *after* a known edge, or taken whole, so
    /// they need no back link.
    next_in: Option<EdgeId<'a>>,
    /// Whether a seq edge survives into the document; decided by the
    /// emitter's counting walk.
    pub(crate) kept: bool,
}

/// A scope while its line is being built: where it opened, and where it
/// closed once it has.
#[derive(Copy, Clone)]
struct Slot<'a> {
    kind: Scope,
    from: NodeId<'a>,
    to: Option<NodeId<'a>>,
}

/// The scope graph of one line; every buffer is reused across lines.
pub(crate) struct Graph<'a> {
    /// One node per item of the line, in order.
    pub(crate) nodes: Arena<Node<'a>>,
    /// The shared edge arena the intrusive lists thread through.
    pub(crate) edges: Arena<Edge<'a>>,
    // Build scratch: the scopes of the line in opening order, and the
    // indices of those still open.
    slots: Vec<Slot<'a>>,
    open: Vec<usize>,
}

impl<'a> Graph<'a> {
    pub(crate) fn new() -> Self {
        Graph {
            nodes: Arena::new(),
            edges: Arena::new(),
            slots: Vec::new(),
            open: Vec::new(),
        }
    }

    /// Reads the line's items off its events — a breakable composition ends
    /// an item — and replays each scope open and close at the node of the
    /// item it falls in. A run's internal comps and the scopes that open or
    /// close before its trailing separator all share the run's node,
    /// exactly as document order threads them.
    ///
    /// The open scopes form a stack (a close pops from it, an open pushes
    /// onto it), and scopes open in document pre-order, so materializing
    /// edges in opening order gives every node's ins and outs lists in
    /// pre-order.
    pub(crate) fn build(&mut self, line: &Line<'_, 'a>) {
        self.nodes.clear();
        self.edges.clear();
        self.slots.clear();
        self.open.clear();
        let mut start = 0;
        let mut node = self.nodes.push(Node::over(Range::new(0, 0)));
        for (i, event) in line.events.iter().enumerate() {
            match *event {
                Event::Text { .. } | Event::Comp(_, Break::Fixed) => {}
                Event::Open(kind) => {
                    self.open.push(self.slots.len());
                    self.slots.push(Slot {
                        kind,
                        from: node,
                        to: None,
                    });
                }
                Event::Close => {
                    let slot = self.open.pop().expect("a closed scope was opened");
                    self.slots[slot].to = Some(node);
                }
                Event::Comp(_, Break::Breakable) => {
                    self.nodes[node].events = Range::new(start, i);
                    start = i + 1;
                    node = self.nodes.push(Node::over(Range::new(start, start)));
                }
            }
        }
        self.nodes[node].events = Range::new(start, line.events.len());
        assert!(
            self.open.is_empty(),
            "every scope closes by the end of its line"
        );
        for i in 0..self.slots.len() {
            let slot = self.slots[i];
            let to = slot.to.expect("every scope closes by the end of its line");
            if slot.from != to {
                self.add_edge(slot.kind, slot.from, to);
            }
        }
    }

    /// Appends an edge to its source's outs list and its target's ins list.
    fn add_edge(&mut self, kind: Scope, source: NodeId<'a>, target: NodeId<'a>) {
        let edge = self.edges.push(Edge {
            kind,
            source,
            target,
            next_out: None,
            prev_out: None,
            next_in: None,
            kept: false,
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
    pub(crate) fn solve(&mut self) {
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
                Scope::Grp => break Some(curr),
                Scope::Seq => {
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
