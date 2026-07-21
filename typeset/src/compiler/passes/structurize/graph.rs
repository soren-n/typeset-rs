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

use crate::compiler::passes::term_chain::{TermChain, TermSink, TermStep};
use bumpalo::Bump;
use std::cell::Cell;

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
    pub term: &'a GraphTerm<'a>,
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

#[derive(Debug)]
pub(super) enum GraphTerm<'a> {
    Null,
    Text(&'a str),
    Fix(&'a GraphFix<'a>),
    Nest(&'a GraphTerm<'a>),
    Pack(u64, &'a GraphTerm<'a>),
}

#[derive(Debug)]
pub(super) enum GraphFix<'a> {
    Last(&'a GraphTerm<'a>),
    Next(&'a GraphTerm<'a>, &'a GraphFix<'a>, bool),
}

/// Per-node data `rebuild` reads from the solved graph: the node's term, its
/// in-degree, and its out-properties. Owned and transient, one per node in
/// node-index order.
#[derive(Debug)]
pub(super) struct NodeInfo<'a> {
    pub term: &'a GraphTerm<'a>,
    pub in_degree: u64,
    pub outs: Vec<Property>,
}

// `GraphTerm` participates in the shared nest/pack term-chain mapping: graphify
// builds one from a `Term` (sink), rebuild reads one back into a `Term` (chain).
impl<'a> TermChain<'a> for GraphTerm<'a> {
    fn step(&'a self) -> TermStep<'a, Self> {
        match self {
            GraphTerm::Null => TermStep::Null,
            GraphTerm::Text(data) => TermStep::Text(data),
            GraphTerm::Nest(term1) => TermStep::Nest(term1),
            GraphTerm::Pack(index, term1) => TermStep::Pack(*index, term1),
            GraphTerm::Fix(_fix) => unreachable!("Invariant"),
        }
    }
}

impl<'b> TermSink<'b> for GraphTerm<'b> {
    fn null(mem: &'b Bump) -> &'b Self {
        mem.alloc(GraphTerm::Null)
    }
    fn text(mem: &'b Bump, data: &'b str) -> &'b Self {
        mem.alloc(GraphTerm::Text(data))
    }
    fn nest(mem: &'b Bump, inner: &'b Self) -> &'b Self {
        mem.alloc(GraphTerm::Nest(inner))
    }
    fn pack(mem: &'b Bump, index: u64, inner: &'b Self) -> &'b Self {
        mem.alloc(GraphTerm::Pack(index, inner))
    }
}
