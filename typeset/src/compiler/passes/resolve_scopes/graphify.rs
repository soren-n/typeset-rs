//! Phase 1 of resolve_scopes: FixedDoc → GraphDoc.
//!
//! Walks each line, assigns a node index per item, and materializes the
//! grp/seq scopes as graph edges (the scope graph that `solve` then resolves).
//! Items are not copied — a graph node is just its line's like-indexed item,
//! so the graph borrows the `FixedDoc` and stores only edge structure.
//!
//! Each composition carries the scopes that *open* and *close* at it (computed
//! back in `serialize`). Replaying those deltas per line — open records a
//! scope's `from` node, close pairs it with a `to` node and emits an edge — is
//! linear in the number of scopes, so deeply nested grp/seq no longer cost
//! O(n^2) (the old full-stack diff did).

use super::graph::{EdgeData, GraphDoc, GraphLine, NONE, NodeData, Property};
use crate::compiler::types::{FixedComp, FixedDoc, FixedItem, FixedLine, Scope};
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
// that begins here. The deltas are ranges into the serial document's shared
// scope buffer.
fn apply_comp(
    node: u32,
    comp: &FixedComp,
    scopes: &[Scope],
    open: &mut OpenScopes,
    edges: &mut Vec<Edge>,
) {
    for scope in comp.closes.slice(scopes) {
        let index = scope_index(scope);
        let (prop, from) = open
            .remove(&index)
            .expect("Invariant: scope closed without a matching open");
        edges.push((index, prop, from, node));
    }
    for scope in comp.opens.slice(scopes) {
        open.insert(scope_index(scope), (scope_prop(scope), node));
    }
}

pub(super) fn graphify<'b, 'a>(doc: &'b FixedDoc<'a>, scopes: &[Scope]) -> GraphDoc<'b, 'a> {
    // Every item across all lines is one node.
    let node_total = doc.items.len();
    let mut g = GraphDoc {
        fixed: doc,
        lines: Vec::with_capacity(doc.lines.len()),
        nodes: Vec::with_capacity(node_total),
        edges: Vec::new(),
    };
    // Per-line edge scratch, reused across lines.
    let mut edges: Vec<Edge> = Vec::new();
    // `doc` is passed alongside `g` (not read back through `g.fixed`) so that
    // resolving a line's item/sep ranges borrows `doc` while `g.nodes` is
    // pushed to — two disjoint borrows.
    for &line in &doc.lines {
        visit_line(&mut g, doc, line, scopes, &mut edges);
    }
    g
}

/// Graphifies one line: assigns a node index per item and replays each
/// composition's scope deltas at that index. A fix item's internal comps and
/// its trailing separator all share the item's index, exactly as document
/// order threads them.
fn visit_line<'b, 'a>(
    g: &mut GraphDoc<'b, 'a>,
    doc: &FixedDoc<'a>,
    line: FixedLine,
    scopes: &[Scope],
    edges: &mut Vec<Edge>,
) {
    let start = g.nodes.len() as u32;
    let mut open: OpenScopes = BTreeMap::new();
    edges.clear();
    let items = line.items.slice(&doc.items);
    let seps = line.seps.slice(&doc.item_seps);
    for (i, item) in items.iter().enumerate() {
        let index = start + i as u32;
        g.nodes.push(NodeData::new());
        if let FixedItem::Fix(run) = item {
            for sep in run.seps.slice(&doc.run_seps) {
                apply_comp(index, sep, scopes, &mut open, edges);
            }
        }
        if let Some(sep) = seps.get(i) {
            apply_comp(index, sep, scopes, &mut open, edges);
        }
    }
    // Close every scope still open at the line's last node.
    let last_index = start + (items.len() - 1) as u32;
    for (index, (prop, from)) in &open {
        edges.push((*index, *prop, *from, last_index));
    }
    // Materialize edges in scope-index order, so each node's ins/outs lists
    // are ordered exactly as the old index-keyed map produced them (solve
    // and rebuild depend on that order).
    edges.sort_by_key(|(index, ..)| *index);
    for &(_index, prop, from, to) in edges.iter() {
        if from != to {
            let id = g.edges.len() as u32;
            g.edges.push(EdgeData {
                prop,
                source: from,
                target: to,
                next_out: NONE,
                prev_out: NONE,
                next_in: NONE,
            });
            g.append_out(from, id);
            g.append_in(to, id);
        }
    }
    g.lines.push(GraphLine {
        line,
        nodes_start: start,
        nodes_end: g.nodes.len() as u32,
    });
}
