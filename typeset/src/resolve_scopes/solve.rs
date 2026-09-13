//! Phase 2 of resolve_scopes: resolve the scope graph in place.
//!
//! For each node with both incoming and outgoing scope edges, the outgoing seq
//! edges are moved out of the way (re-sourced onto the incoming side) and the
//! incoming edges are handed forward past the first outgoing grp edge, leaving
//! a graph the rebuild phase can walk as a plain composition spine.
//!
//! The incident-edge lists are intrusive linked lists through the shared edge
//! arena, so each move — pop an outs head, insert before a known edge, splice a
//! whole ins list in after a known edge — is O(1) pointer rewiring.

use super::graph::{EdgeId, GraphDoc, NodeId};
use crate::serialize::ScopeKind;

/// Edges never cross lines, so the per-line resolution loop is one pass over
/// the document-wide node arena.
pub(super) fn solve(g: &mut GraphDoc<'_, '_>) {
    for node in g.nodes.ids() {
        solve_node(g, node);
    }
}

fn solve_node(g: &mut GraphDoc<'_, '_>, node: NodeId) {
    let (Some(ins_head), Some(_)) = (g.nodes[node].ins_head, g.nodes[node].outs_head) else {
        return;
    };

    // The incoming edge whose source is leftmost (first on ties: forward
    // iteration only replaces on strictly smaller sources).
    let mut ins_first = ins_head;
    let mut best_src = g.edges[ins_head].source;
    let mut e = g.edges[ins_head].next_in;
    while let Some(edge) = e {
        let src = g.edges[edge].source;
        if src < best_src {
            best_src = src;
            ins_first = edge;
        }
        e = g.edges[edge].next_in;
    }

    // Walk this node's outgoing edges, moving each leading seq edge out of
    // the way, until the first grp edge (or the end). A moved seq edge is
    // re-sourced onto the working edge's source, inserted immediately
    // before it, and becomes the working edge itself — so successive seq
    // edges stack up in front of `ins_first`.
    let mut edge = ins_first;
    let grp = loop {
        let Some(curr) = g.nodes[node].outs_head else {
            break None;
        };
        match g.edges[curr].kind {
            ScopeKind::Grp => break Some(curr),
            ScopeKind::Seq => {
                pop_out_head(g, node);
                let src = g.edges[edge].source;
                insert_out_before(g, src, curr, edge);
                g.edges[curr].source = src;
                edge = curr;
            }
        }
    };

    // Hand this node's whole incoming list forward past the grp edge:
    // retarget every incoming edge to the grp's target and splice the list
    // immediately after the grp edge in that target's ins list.
    if let Some(grp) = grp {
        let head = g.nodes[node].ins_head;
        let tail = g.nodes[node].ins_tail.expect("ins list has a tail");
        let len = g.nodes[node].ins_len;
        g.nodes[node].ins_head = None;
        g.nodes[node].ins_tail = None;
        g.nodes[node].ins_len = 0;

        let target = g.edges[grp].target;
        let mut e = head;
        while let Some(edge) = e {
            g.edges[edge].target = target;
            e = g.edges[edge].next_in;
        }

        let after = g.edges[grp].next_in;
        g.edges[grp].next_in = head;
        g.edges[tail].next_in = after;
        if after.is_none() {
            g.nodes[target].ins_tail = Some(tail);
        }
        g.nodes[target].ins_len += len;
    }
}

/// Detaches the head edge of `node`'s outs list.
fn pop_out_head(g: &mut GraphDoc<'_, '_>, node: NodeId) {
    let head = g.nodes[node].outs_head.expect("outs list is non-empty");
    let next = g.edges[head].next_out;
    g.nodes[node].outs_head = next;
    match next {
        None => g.nodes[node].outs_tail = None,
        Some(next) => g.edges[next].prev_out = None,
    }
    g.edges[head].next_out = None;
}

/// Inserts `new` immediately before `before` in `src`'s outs list.
fn insert_out_before(g: &mut GraphDoc<'_, '_>, src: NodeId, new: EdgeId, before: EdgeId) {
    let prev = g.edges[before].prev_out;
    g.edges[new].prev_out = prev;
    g.edges[new].next_out = Some(before);
    g.edges[before].prev_out = Some(new);
    match prev {
        None => g.nodes[src].outs_head = Some(new),
        Some(prev) => g.edges[prev].next_out = Some(new),
    }
}
