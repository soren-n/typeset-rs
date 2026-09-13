//! Phase 1 of resolve_scopes: FixedDoc → GraphDoc.
//!
//! Walks each line, assigns a node per item, and materializes the grp/seq
//! scopes as graph edges (the scope graph that `solve` then resolves). Items
//! are not copied — a graph node is just its line's like-indexed item, so the
//! graph borrows the `FixedDoc` and stores only edge structure.
//!
//! Each composition carries the scopes that *open* and *close* at it (computed
//! back in `serialize`). Replaying those deltas per line — open records a
//! scope's `from` node, close pairs it with a `to` node and emits an edge — is
//! linear in the number of scopes.

use super::graph::{EdgeData, GraphDoc, GraphLine, NodeData, NodeId};
use crate::compiler::passes::serialize::{FixedComp, FixedDoc, FixedItem, FixedLine};
use crate::compiler::types::{Arena, Range, Scope, ScopeKind};
use std::collections::BTreeMap;

// The scopes open across the current point of a line, keyed by scope index:
// each records the scope's kind and the node it opened at.
type OpenScopes = BTreeMap<u32, (ScopeKind, NodeId)>;
// A resolved scope edge: (scope index, kind, from node, to node). Collected per
// line, then materialized in scope-index order.
type Edge = (u32, ScopeKind, NodeId, NodeId);

// Applies one composition's scope deltas at `node`: close each scope that ends
// here (pairing it with its recorded open into an edge), then open each scope
// that begins here. The deltas are ranges into the serial document's shared
// scope buffer.
fn apply_comp(
    node: NodeId,
    comp: &FixedComp,
    scopes: &[Scope],
    open: &mut OpenScopes,
    edges: &mut Vec<Edge>,
) {
    for scope in comp.closes.slice(scopes) {
        let (kind, from) = open
            .remove(&scope.index)
            .expect("Invariant: scope closed without a matching open");
        edges.push((scope.index, kind, from, node));
    }
    for scope in comp.opens.slice(scopes) {
        open.insert(scope.index, (scope.kind, node));
    }
}

pub(super) fn graphify<'b, 'a>(doc: &'b FixedDoc<'a>) -> GraphDoc<'b, 'a> {
    // Every item across all lines is one node.
    let mut g = GraphDoc {
        fixed: doc,
        lines: Vec::with_capacity(doc.lines.len()),
        nodes: Arena::with_capacity(doc.items.len()),
        edges: Arena::new(),
    };
    // Per-line edge scratch, reused across lines.
    let mut edges: Vec<Edge> = Vec::new();
    // `doc` is passed alongside `g` (not read back through `g.fixed`) so that
    // resolving a line's item/sep ranges borrows `doc` while `g.nodes` is
    // pushed to — two disjoint borrows.
    for &line in &doc.lines {
        visit_line(&mut g, doc, line, &mut edges);
    }
    g
}

/// Graphifies one line: assigns a node per item and replays each
/// composition's scope deltas at that node. A fix item's internal comps and
/// its trailing separator all share the item's node, exactly as document
/// order threads them.
fn visit_line<'b, 'a>(
    g: &mut GraphDoc<'b, 'a>,
    doc: &FixedDoc<'a>,
    line: FixedLine<'a>,
    edges: &mut Vec<Edge>,
) {
    let scopes = &doc.scopes;
    let start = g.nodes.len();
    let mut open: OpenScopes = BTreeMap::new();
    edges.clear();
    let items = line.items.slice(&doc.items);
    let seps = line.seps.slice(&doc.item_seps);
    for (i, item) in items.iter().enumerate() {
        let node = g.nodes.push(NodeData::new());
        if let FixedItem::Fix(run) = item {
            for sep in run.seps.slice(&doc.run_seps) {
                apply_comp(node, sep, scopes, &mut open, edges);
            }
        }
        if let Some(sep) = seps.get(i) {
            apply_comp(node, sep, scopes, &mut open, edges);
        }
    }
    let nodes = Range::new(start, g.nodes.len());
    // Close every scope still open at the line's last node.
    let last = nodes.id_at(nodes.len() - 1);
    for (index, (kind, from)) in &open {
        edges.push((*index, *kind, *from, last));
    }
    // Materialize edges in scope-index order: each node's ins/outs lists are
    // ordered by scope index, which solve and rebuild depend on.
    edges.sort_by_key(|(index, ..)| *index);
    for &(_index, kind, from, to) in edges.iter() {
        if from != to {
            let id = g.edges.push(EdgeData {
                kind,
                source: from,
                target: to,
                next_out: None,
                prev_out: None,
                next_in: None,
            });
            g.append_out(from, id);
            g.append_in(to, id);
        }
    }
    g.lines.push(GraphLine { line, nodes });
}
