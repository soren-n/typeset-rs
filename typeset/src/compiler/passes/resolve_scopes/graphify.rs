//! Phase 1 of structurize: FixedDoc → GraphDoc.
//!
//! Walks each line, assigns a node index per item, and materializes the
//! grp/seq scopes as graph edges (the scope graph that `solve` then resolves).
//!
//! Each composition carries the scopes that *open* and *close* at it (computed
//! back in `serialize`). Replaying those deltas per line — open records a
//! scope's `from` node, close pairs it with a `to` node and emits an edge — is
//! linear in the number of scopes, so deeply nested grp/seq no longer cost
//! O(n^2) (the old full-stack diff did).

use super::graph::{EdgeData, GraphDoc, GraphFixRun, LineGraph, NodeData, NodeItem, Property};
use crate::compiler::types::{FixRun, FixedComp, FixedDoc, FixedItem, FixedLine, Scope};
use std::collections::BTreeMap;

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
type OpenScopes = BTreeMap<u64, (Property, u32)>;
// A resolved scope edge: (scope index, kind, from node, to node). Collected per
// line, then materialized in scope-index order.
type Edge = (u64, Property, u32, u32);

// Applies one composition's scope deltas at `node`: close each scope that ends
// here (pairing it with its recorded open into an edge), then open each scope
// that begins here. Returns the composition's pad flag.
fn apply_comp(node: u32, comp: &FixedComp, open: &mut OpenScopes, edges: &mut Vec<Edge>) -> bool {
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

pub(super) fn graphify<'a>(doc: &FixedDoc<'a>) -> GraphDoc<'a> {
    doc.lines.iter().map(visit_line).collect()
}

/// Graphifies one line: assigns a node index per item and replays each
/// composition's scope deltas at that index. A fix item's internal comps and
/// its trailing separator all share the item's index, exactly as document
/// order threads them.
fn visit_line<'a>(line: &FixedLine<'a>) -> LineGraph<'a> {
    let mut nodes: Vec<NodeData<'a>> = Vec::with_capacity(line.items.len());
    let mut pads: Vec<bool> = Vec::with_capacity(line.seps.len());
    let mut open: OpenScopes = BTreeMap::new();
    let mut edges: Vec<Edge> = Vec::new();
    for (i, item) in line.items.iter().enumerate() {
        let index = i as u32;
        let item1 = match item {
            FixedItem::Term(term) => NodeItem::Term(term),
            FixedItem::Fix(run) => NodeItem::Fix(visit_fix(run, index, &mut open, &mut edges)),
        };
        nodes.push(NodeData {
            item: item1,
            ins: Vec::new(),
            outs: Vec::new(),
        });
        if let Some(sep) = line.seps.get(i) {
            pads.push(apply_comp(index, sep, &mut open, &mut edges));
        }
    }
    // Close every scope still open at the line's last node.
    let last_index = (line.items.len() - 1) as u32;
    for (index, (prop, from)) in &open {
        edges.push((*index, *prop, *from, last_index));
    }
    // Materialize edges in scope-index order, so each node's ins/outs lists
    // are ordered exactly as the old index-keyed map produced them (solve
    // and rebuild depend on that order).
    edges.sort_by_key(|(index, ..)| *index);
    let mut edge_data: Vec<EdgeData> = Vec::new();
    for (_index, prop, from, to) in edges {
        if from != to {
            let id = edge_data.len() as u32;
            edge_data.push(EdgeData {
                prop,
                source: from,
                target: to,
            });
            nodes[from as usize].outs.push(id);
            nodes[to as usize].ins.push(id);
        }
    }
    LineGraph {
        nodes,
        edges: edge_data,
        pads,
    }
}

/// Replays a fix run's internal scope deltas at the run's node index and keeps
/// the pads; the terms pass through by borrow.
fn visit_fix<'a>(
    run: &FixRun<'a>,
    index: u32,
    open: &mut OpenScopes,
    edges: &mut Vec<Edge>,
) -> GraphFixRun<'a> {
    let pads: Vec<bool> = run
        .seps
        .iter()
        .map(|sep| apply_comp(index, sep, open, edges))
        .collect();
    GraphFixRun {
        terms: run.terms.clone(),
        pads,
    }
}
