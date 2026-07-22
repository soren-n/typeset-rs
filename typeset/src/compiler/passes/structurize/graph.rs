//! The scope graph that the three structurize phases share.
//!
//! `graphify` builds it from a `FixedDoc`, `solve` resolves it in place, and
//! `rebuild` reads it back into a `RebuildDoc`. Nothing outside structurize
//! touches these types, so they live here rather than in the shared IR module.
//!
//! Each line is an array of [`GraphNode`]s in index order. Grp/seq scopes are
//! [`GraphEdge`]s; every edge sits on two intrusive doubly-linked lists at once
//! — its target's incoming list (`ins_*`) and its source's outgoing list
//! (`outs_*`) — which is what lets `solve` splice edges around in O(1).

use crate::compiler::types::Term;
use bumpalo::Bump;
use std::cell::Cell;

/// One line of the graph spine: its nodes in index order and the pad flags
/// between adjacent nodes (`pads[i]` is the pad between node `i` and `i + 1`).
pub(super) type GraphLine<'a> = (&'a [&'a GraphNode<'a>], &'a [bool]);

/// Collect the lines of a `GraphDoc` spine, in order, into a `Vec`.
///
/// The `Eod`/`Break` spine is linear, so this is a plain loop — the shared
/// counterpart to [`build_graph_doc`], used by `solve` and `rebuild` to walk the
/// spine without recursing.
pub(super) fn graph_lines<'a>(doc: &'a GraphDoc<'a>) -> Vec<GraphLine<'a>> {
    let mut lines: Vec<GraphLine<'a>> = Vec::new();
    let mut cur = doc;
    while let GraphDoc::Break(nodes, pads, rest) = cur {
        lines.push((nodes, pads));
        cur = rest;
    }
    lines
}

/// Fold a slice of lines (document order) onto a fresh `Eod` terminal to build a
/// `GraphDoc` spine. The shared counterpart to [`graph_lines`], used by
/// `graphify` and `solve` to assemble their output spine.
pub(super) fn build_graph_doc<'a>(mem: &'a Bump, lines: &[GraphLine<'a>]) -> &'a GraphDoc<'a> {
    let mut gdoc: &'a GraphDoc<'a> = mem.alloc(GraphDoc::Eod);
    for &(nodes, pads) in lines.iter().rev() {
        gdoc = mem.alloc(GraphDoc::Break(nodes, pads, gdoc));
    }
    gdoc
}

/// The kind of a grp or seq scope edge. (`graphify` tracks scope *indices*
/// while building the graph via the separate `Scope` type from the shared IR;
/// once an edge is materialized only its kind matters, which is this.)
#[derive(Debug, Copy, Clone)]
pub(super) enum Property {
    Grp,
    Seq,
}

#[derive(Debug)]
pub(super) enum GraphDoc<'a> {
    Eod,
    Break(&'a [&'a GraphNode<'a>], &'a [bool], &'a GraphDoc<'a>),
}

#[derive(Debug)]
pub(super) struct GraphNode<'a> {
    pub index: u64,
    pub item: GraphItem<'a>,
    pub ins_head: Cell<Option<&'a GraphEdge<'a>>>,
    pub ins_tail: Cell<Option<&'a GraphEdge<'a>>>,
    pub outs_head: Cell<Option<&'a GraphEdge<'a>>>,
    pub outs_tail: Cell<Option<&'a GraphEdge<'a>>>,
}

#[derive(Debug)]
pub(super) struct GraphEdge<'a> {
    pub prop: Property,
    pub ins_next: Cell<Option<&'a GraphEdge<'a>>>,
    pub ins_prev: Cell<Option<&'a GraphEdge<'a>>>,
    pub outs_next: Cell<Option<&'a GraphEdge<'a>>>,
    pub outs_prev: Cell<Option<&'a GraphEdge<'a>>>,
    pub source: Cell<&'a GraphNode<'a>>,
    pub target: Cell<&'a GraphNode<'a>>,
}

/// A graph node's payload: either a plain term (borrowed from the serialize
/// arena — terms are invariant from serialize through structurize) or a
/// coalesced fixed group.
#[derive(Debug, Copy, Clone)]
pub(super) enum GraphItem<'a> {
    Term(&'a Term<'a>),
    Fix(&'a GraphFix<'a>),
}

#[derive(Debug)]
pub(super) enum GraphFix<'a> {
    Last(&'a Term<'a>),
    Next(&'a Term<'a>, &'a GraphFix<'a>, bool),
}

/// Per-node data `rebuild` reads from the solved graph: the node's item, its
/// in-degree, and its out-properties. Owned and transient, one per node in
/// node-index order.
#[derive(Debug)]
pub(super) struct NodeInfo<'a> {
    pub item: GraphItem<'a>,
    pub in_degree: u64,
    pub outs: Vec<Property>,
}
