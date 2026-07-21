//! Phase 1 of structurize: FixedDoc → GraphDoc.
//!
//! Walks the fixed spine, assigns a node index per item, and materializes the
//! grp/seq scopes as graph edges (the scope graph that `solve` then resolves).
//!
//! Each composition carries the scopes that *open* and *close* at it (computed
//! back in `serialize`). Replaying those deltas per line — open records a
//! scope's `from` node, close pairs it with a `to` node and emits an edge — is
//! linear in the number of scopes, so deeply nested grp/seq no longer cost
//! O(n^2) (the old full-stack diff did). `solve` and `rebuild` are unchanged.

use super::graph::{
    GraphDoc, GraphEdge, GraphFix, GraphLine, GraphNode, GraphTerm, Property, build_graph_doc,
};
use crate::compiler::passes::term_chain::map_term_chain;
use crate::compiler::types::{FixedComp, FixedDoc, FixedFix, FixedItem, FixedObj, Scope};
use bumpalo::Bump;
use std::cell::Cell;
use std::collections::BTreeMap;

// Helper function to create graph nodes
fn make_node<'a>(mem: &'a Bump, index: u64, term: &'a GraphTerm<'a>) -> &'a GraphNode<'a> {
    mem.alloc(GraphNode {
        index,
        term,
        ins_head: Cell::new(None),
        ins_tail: Cell::new(None),
        outs_head: Cell::new(None),
        outs_tail: Cell::new(None),
    })
}

// Helper function to create graph edges
fn make_edge<'a>(
    mem: &'a Bump,
    prop: Property,
    source: &'a GraphNode<'a>,
    target: &'a GraphNode<'a>,
) -> &'a GraphEdge<'a> {
    mem.alloc(GraphEdge {
        prop,
        ins_next: Cell::new(None),
        ins_prev: Cell::new(None),
        outs_next: Cell::new(None),
        outs_prev: Cell::new(None),
        source: Cell::new(source),
        target: Cell::new(target),
    })
}

fn scope_index(scope: &Scope) -> u64 {
    match scope {
        Scope::Grp(index) | Scope::Seq(index) => *index,
    }
}

fn scope_prop(scope: &Scope) -> Property {
    match scope {
        Scope::Grp(_) => Property::Grp,
        Scope::Seq(_) => Property::Seq,
    }
}

// The scopes open across the current point of a line, keyed by scope index:
// each records the scope's kind and the node it opened at.
type OpenScopes = BTreeMap<u64, (Property, u64)>;
// A resolved scope edge: (scope index, kind, from node, to node). Collected per
// line, then materialized in scope-index order.
type Edge = (u64, Property, u64, u64);

// Applies one composition's scope deltas at `node`: close each scope that ends
// here (pairing it with its recorded open into an edge), then open each scope
// that begins here. Returns the composition's pad flag.
fn apply_comp(node: u64, comp: &FixedComp, open: &mut OpenScopes, edges: &mut Vec<Edge>) -> bool {
    let FixedComp { pad, opens, closes } = comp;
    for scope in closes.iter() {
        let index = scope_index(scope);
        let (prop, from) = open
            .remove(&index)
            .expect("Invariant: scope closed without a matching open");
        edges.push((index, prop, from, node));
    }
    for scope in opens.iter() {
        open.insert(scope_index(scope), (scope_prop(scope), node));
    }
    *pad
}

fn push_ins<'a>(edge: &'a GraphEdge<'a>, node: &'a GraphNode<'a>) {
    match node.ins_tail.get() {
        None => {
            node.ins_head.set(Some(edge));
            node.ins_tail.set(Some(edge))
        }
        Some(tail) => {
            edge.ins_prev.set(Some(tail));
            tail.ins_next.set(Some(edge));
            node.ins_tail.set(Some(edge))
        }
    }
}

fn push_outs<'a>(edge: &'a GraphEdge<'a>, node: &'a GraphNode<'a>) {
    match node.outs_tail.get() {
        None => {
            node.outs_head.set(Some(edge));
            node.outs_tail.set(Some(edge))
        }
        Some(tail) => {
            edge.outs_prev.set(Some(tail));
            tail.outs_next.set(Some(edge));
            node.outs_tail.set(Some(edge))
        }
    }
}

pub(super) fn graphify<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b GraphDoc<'b> {
    fn visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b GraphDoc<'b> {
        // Walk the linear FixedDoc spine, graphifying each line's object, then
        // fold the resulting lines into a GraphDoc spine.
        let mut breaks: Vec<GraphLine<'b>> = Vec::new();
        let mut cur = doc;
        loop {
            match cur {
                FixedDoc::Eod => break,
                FixedDoc::Break(obj, doc1) => {
                    breaks.push(visit_obj(mem, obj));
                    cur = doc1;
                }
            }
        }
        build_graph_doc(mem, &breaks)
    }
    fn visit_obj<'b, 'a: 'b>(
        mem: &'b Bump,
        obj: &'a FixedObj<'a>,
    ) -> (&'b [&'b GraphNode<'b>], &'b [bool]) {
        // Walk the object's item chain, assigning a node index per item and
        // replaying each composition's scope deltas at that index. A fix item's
        // internal comps and its trailing separator all share the item's index,
        // exactly as document order threads them.
        let mut nodes_vec: Vec<&'b GraphNode<'b>> = Vec::new();
        let mut pads_vec: Vec<bool> = Vec::new();
        let mut open: OpenScopes = BTreeMap::new();
        let mut edges: Vec<Edge> = Vec::new();
        let mut index: u64 = 0;
        let mut cur = obj;
        let last_index: u64 = loop {
            match cur {
                FixedObj::Next(item, comp, obj1) => {
                    let term1 = match item {
                        FixedItem::Term(term) => map_term_chain(mem, *term),
                        FixedItem::Fix(fix) => {
                            let fix1 = visit_fix(mem, fix, index, &mut open, &mut edges);
                            mem.alloc(GraphTerm::Fix(fix1))
                        }
                    };
                    nodes_vec.push(make_node(mem, index, term1));
                    pads_vec.push(apply_comp(index, comp, &mut open, &mut edges));
                    index += 1;
                    cur = obj1;
                }
                FixedObj::Last(item) => {
                    let term1 = match item {
                        FixedItem::Term(term) => map_term_chain(mem, *term),
                        FixedItem::Fix(fix) => {
                            let fix1 = visit_fix(mem, fix, index, &mut open, &mut edges);
                            mem.alloc(GraphTerm::Fix(fix1))
                        }
                    };
                    nodes_vec.push(make_node(mem, index, term1));
                    break index;
                }
            }
        };
        // Close every scope still open at the line's last node.
        for (index, (prop, from)) in &open {
            edges.push((*index, *prop, *from, last_index));
        }
        // Materialize edges in scope-index order, so each node's ins/outs lists
        // are ordered exactly as the old index-keyed map produced them (solve
        // and rebuild depend on that order).
        edges.sort_by_key(|(index, ..)| *index);
        let nodes = mem.alloc_slice_copy(&nodes_vec);
        for (_index, prop, from, to) in edges {
            if from != to {
                let from_node = nodes[from as usize];
                let to_node = nodes[to as usize];
                let edge = make_edge(mem, prop, from_node, to_node);
                push_ins(edge, to_node);
                push_outs(edge, from_node);
            }
        }
        (nodes, mem.alloc_slice_copy(&pads_vec))
    }
    fn visit_fix<'b, 'a: 'b>(
        mem: &'b Bump,
        fix: &'a FixedFix<'a>,
        index: u64,
        open: &mut OpenScopes,
        edges: &mut Vec<Edge>,
    ) -> &'b GraphFix<'b> {
        // Walk the fix chain forward, replaying each internal comp's deltas at
        // the fix item's node index; rebuild the GraphFix bottom-up.
        let mut recorded: Vec<(&'b GraphTerm<'b>, bool)> = Vec::new();
        let mut cur = fix;
        let last_term: &'b GraphTerm<'b> = loop {
            match cur {
                FixedFix::Next(term, comp, fix1) => {
                    let term1 = map_term_chain(mem, *term);
                    let pad = apply_comp(index, comp, open, edges);
                    recorded.push((term1, pad));
                    cur = fix1;
                }
                FixedFix::Last(term) => break map_term_chain(mem, *term),
            }
        };
        let mut gfix: &'b GraphFix<'b> = mem.alloc(GraphFix::Last(last_term));
        for &(term1, pad) in recorded.iter().rev() {
            gfix = mem.alloc(GraphFix::Next(term1, gfix, pad));
        }
        gfix
    }
    visit_doc(mem, doc)
}
