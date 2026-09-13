//! The scope graph that the three resolve_scopes phases share.
//!
//! `graphify` builds it from a `FixedDoc`, `solve` resolves it in place, and
//! `rebuild` reads it back into a `RebuildDoc`. Nothing outside resolve_scopes
//! touches these types, so they live here rather than in the shared IR module.
//!
//! The whole document shares one node arena and one edge arena; each line owns
//! a contiguous node range (nodes in document order, index-aligned with the
//! line's items, which stay borrowed from the `FixedDoc` rather than copied).
//! A node's incident edges are intrusive linked lists threaded through the
//! edge arena — each edge sits in its source's `outs` list and its target's
//! `ins` list — so `solve`'s surgery (pop a list head, insert before a known
//! edge, splice one list into another) is O(1) pointer rewiring and building
//! the graph allocates nothing per node or edge.

use crate::compiler::passes::serialize::{FixedDoc, FixedLine};
use crate::compiler::types::{Arena, Id, Range, ScopeKind};

pub(super) type NodeId = Id<NodeData>;
pub(super) type EdgeId = Id<EdgeData>;

/// A node's ends of the intrusive edge lists. The node's payload is the
/// like-indexed item of its line (nodes are index-aligned with `line.items`),
/// so nothing else is stored here.
#[derive(Debug)]
pub(super) struct NodeData {
    /// Edges targeting this node, in list order (solve depends on the order).
    pub ins_head: Option<EdgeId>,
    pub ins_tail: Option<EdgeId>,
    /// Number of edges on the ins list.
    pub ins_len: u32,
    /// Edges sourced at this node, in list order (solve and rebuild depend on
    /// the order).
    pub outs_head: Option<EdgeId>,
    pub outs_tail: Option<EdgeId>,
}

impl NodeData {
    pub fn new() -> NodeData {
        NodeData {
            ins_head: None,
            ins_tail: None,
            ins_len: 0,
            outs_head: None,
            outs_tail: None,
        }
    }
}

#[derive(Debug)]
pub(super) struct EdgeData {
    pub kind: ScopeKind,
    pub source: NodeId,
    pub target: NodeId,
    /// Links within the source's outs list. `prev_out` exists because solve
    /// inserts before an arbitrary known edge of that list.
    pub next_out: Option<EdgeId>,
    pub prev_out: Option<EdgeId>,
    /// Link within the target's ins list. Ins lists are only appended to,
    /// iterated forward, spliced in *after* a known edge, or taken whole, so
    /// they need no back link.
    pub next_in: Option<EdgeId>,
}

/// One line of the graph: the `FixedDoc` line's ranges (items and separator
/// pads are read from the borrowed `FixedDoc`'s buffers) and the line's node
/// range in the shared node arena.
#[derive(Debug)]
pub(super) struct GraphLine<'a> {
    pub line: FixedLine<'a>,
    pub nodes: Range<NodeData>,
}

/// The whole document's scope graph.
#[derive(Debug)]
pub(super) struct GraphDoc<'b, 'a> {
    /// The borrowed `FixedDoc` the lines' ranges index into (its item, term,
    /// and separator buffers back `graphify` and `rebuild`).
    pub fixed: &'b FixedDoc<'a>,
    /// One entry per line, in document order.
    pub lines: Vec<GraphLine<'a>>,
    /// All lines' nodes, contiguous per line, in document order.
    pub nodes: Arena<NodeData>,
    /// The shared edge arena the intrusive lists thread through.
    pub edges: Arena<EdgeData>,
}

impl GraphDoc<'_, '_> {
    /// Appends `edge` to `node`'s outs list (build order).
    pub fn append_out(&mut self, node: NodeId, edge: EdgeId) {
        let tail = self.nodes[node].outs_tail;
        self.edges[edge].prev_out = tail;
        match tail {
            None => self.nodes[node].outs_head = Some(edge),
            Some(tail) => self.edges[tail].next_out = Some(edge),
        }
        self.nodes[node].outs_tail = Some(edge);
    }

    /// Appends `edge` to `node`'s ins list (build order).
    pub fn append_in(&mut self, node: NodeId, edge: EdgeId) {
        match self.nodes[node].ins_tail {
            None => self.nodes[node].ins_head = Some(edge),
            Some(tail) => self.edges[tail].next_in = Some(edge),
        }
        self.nodes[node].ins_tail = Some(edge);
        self.nodes[node].ins_len += 1;
    }
}
