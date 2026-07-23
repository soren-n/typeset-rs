//! The scope graph that the three resolve_scopes phases share.
//!
//! `graphify` builds it from a `FixedDoc`, `solve` resolves it in place, and
//! `rebuild` reads it back into a `RebuildDoc`. Nothing outside resolve_scopes
//! touches these types, so they live here rather than in the shared IR module.
//!
//! The whole document shares one node array and one edge pool; each line owns
//! a contiguous node range (nodes in document order, index-aligned with the
//! line's items, which stay borrowed from the `FixedDoc` rather than copied).
//! A node's incident edges are intrusive linked lists threaded through the
//! edge pool — each edge sits in its source's `outs` list and its target's
//! `ins` list — so `solve`'s surgery (pop a list head, insert before a known
//! edge, splice one list into another) is O(1) pointer rewiring and building
//! the graph allocates nothing per node or edge.

use crate::compiler::types::{FixedDoc, FixedLine};

/// Index of a node in the document-wide node array.
pub(super) type NodeId = u32;

/// Index into the document-wide edge pool.
pub(super) type EdgeId = u32;

/// Sentinel for "no node/edge" in the intrusive lists.
pub(super) const NONE: u32 = u32::MAX;

/// The kind of a grp or seq scope edge. (`graphify` tracks scope *indices*
/// while building the graph via the separate `Scope` type from the shared IR;
/// once an edge is materialized only its kind matters, which is this.)
#[derive(Debug, Copy, Clone)]
pub(super) enum Property {
    Grp,
    Seq,
}

/// A node's ends of the intrusive edge lists. The node's payload is the
/// like-indexed item of its line (nodes are index-aligned with `line.items`),
/// so nothing else is stored here.
#[derive(Debug)]
pub(super) struct NodeData {
    /// Edges targeting this node, in list order (solve depends on the order).
    pub ins_head: EdgeId,
    pub ins_tail: EdgeId,
    /// Number of edges on the ins list.
    pub ins_len: u32,
    /// Edges sourced at this node, in list order (solve and rebuild depend on
    /// the order).
    pub outs_head: EdgeId,
    pub outs_tail: EdgeId,
}

impl NodeData {
    pub fn new() -> NodeData {
        NodeData {
            ins_head: NONE,
            ins_tail: NONE,
            ins_len: 0,
            outs_head: NONE,
            outs_tail: NONE,
        }
    }
}

#[derive(Debug)]
pub(super) struct EdgeData {
    pub prop: Property,
    pub source: NodeId,
    pub target: NodeId,
    /// Links within the source's outs list. `prev_out` exists because solve
    /// inserts before an arbitrary known edge of that list.
    pub next_out: EdgeId,
    pub prev_out: EdgeId,
    /// Link within the target's ins list. Ins lists are only appended to,
    /// iterated forward, spliced in *after* a known edge, or taken whole, so
    /// they need no back link.
    pub next_in: EdgeId,
}

/// One line of the graph: the `FixedDoc` line's ranges (items and separator
/// pads are read from the borrowed `FixedDoc`'s arenas) and the line's node
/// range in the shared node array. `FixedLine` is `Copy` ranges now, so this
/// holds it by value.
#[derive(Debug)]
pub(super) struct GraphLine {
    pub line: FixedLine,
    pub nodes_start: u32,
    pub nodes_end: u32,
}

/// The whole document's scope graph.
#[derive(Debug)]
pub(super) struct GraphDoc<'b, 'a> {
    /// The borrowed `FixedDoc` the lines' ranges index into (its item, term,
    /// and separator arenas back `graphify` and `rebuild`).
    pub fixed: &'b FixedDoc<'a>,
    /// One entry per line, in document order.
    pub lines: Vec<GraphLine>,
    /// All lines' nodes, contiguous per line, in document order.
    pub nodes: Vec<NodeData>,
    /// The shared edge pool the intrusive lists thread through.
    pub edges: Vec<EdgeData>,
}

impl GraphDoc<'_, '_> {
    /// Appends `edge` to `node`'s outs list (build order).
    pub fn append_out(&mut self, node: NodeId, edge: EdgeId) {
        let tail = self.nodes[node as usize].outs_tail;
        self.edges[edge as usize].prev_out = tail;
        if tail == NONE {
            self.nodes[node as usize].outs_head = edge;
        } else {
            self.edges[tail as usize].next_out = edge;
        }
        self.nodes[node as usize].outs_tail = edge;
    }

    /// Appends `edge` to `node`'s ins list (build order).
    pub fn append_in(&mut self, node: NodeId, edge: EdgeId) {
        let tail = self.nodes[node as usize].ins_tail;
        if tail == NONE {
            self.nodes[node as usize].ins_head = edge;
        } else {
            self.edges[tail as usize].next_in = edge;
        }
        self.nodes[node as usize].ins_tail = edge;
        self.nodes[node as usize].ins_len += 1;
    }
}
