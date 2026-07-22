//! The scope graph that the three resolve_scopes phases share.
//!
//! `graphify` builds it from a `FixedDoc`, `solve` resolves it in place, and
//! `rebuild` reads it back into a `RebuildDoc`. Nothing outside resolve_scopes
//! touches these types, so they live here rather than in the shared IR module.
//!
//! Each line is its own graph: nodes in document order, grp/seq scopes as
//! edges. Everything is index-linked — an edge lives in its source's `outs`
//! and its target's `ins` list, both ordered `Vec`s of edge ids — so `solve`
//! moves edges around with plain vector operations and the whole structure is
//! owned (no arena, no interior mutability).

use crate::compiler::types::Term;

/// Index of a node within its line (document order).
pub(super) type NodeId = u32;

/// Index into a [`LineGraph`]'s edge list.
pub(super) type EdgeId = u32;

/// The kind of a grp or seq scope edge. (`graphify` tracks scope *indices*
/// while building the graph via the separate `Scope` type from the shared IR;
/// once an edge is materialized only its kind matters, which is this.)
#[derive(Debug, Copy, Clone)]
pub(super) enum Property {
    Grp,
    Seq,
}

/// A coalesced fixed group: its terms and the pads between adjacent terms.
/// (The scope deltas its separators carried are replayed by `graphify` when
/// this is built; only the pads survive into the graph.)
#[derive(Debug)]
pub(super) struct GraphFixRun<'a> {
    pub terms: Vec<&'a Term<'a>>,
    pub pads: Vec<bool>,
}

/// A graph node's payload: either a plain term (borrowed from the serialize
/// arena — terms are invariant from serialize through resolve_scopes) or a
/// coalesced fixed group.
#[derive(Debug)]
pub(super) enum NodeItem<'a> {
    Term(&'a Term<'a>),
    Fix(GraphFixRun<'a>),
}

#[derive(Debug)]
pub(super) struct NodeData<'a> {
    pub item: NodeItem<'a>,
    /// Edges targeting this node, in list order (solve depends on the order).
    pub ins: Vec<EdgeId>,
    /// Edges sourced at this node, in list order (solve and rebuild depend on
    /// the order).
    pub outs: Vec<EdgeId>,
}

#[derive(Debug)]
pub(super) struct EdgeData {
    pub prop: Property,
    pub source: NodeId,
    pub target: NodeId,
}

/// One line's scope graph, plus the pads between adjacent nodes
/// (`pads[i]` is the pad between node `i` and `i + 1`).
#[derive(Debug)]
pub(super) struct LineGraph<'a> {
    pub nodes: Vec<NodeData<'a>>,
    pub edges: Vec<EdgeData>,
    pub pads: Vec<bool>,
}

/// The whole document's scope graph: one [`LineGraph`] per line, in order.
pub(super) type GraphDoc<'a> = Vec<LineGraph<'a>>;
