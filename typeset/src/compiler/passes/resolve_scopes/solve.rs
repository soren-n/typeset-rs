//! Phase 2 of resolve_scopes: resolve the scope graph in place.
//!
//! For each node with both incoming and outgoing scope edges, the outgoing seq
//! edges are moved out of the way (re-sourced onto the incoming side) and the
//! incoming edges are handed forward past the first outgoing grp edge, leaving
//! a graph the rebuild phase can walk as a plain composition spine.
//!
//! The incident-edge lists are intrusive linked lists through the shared edge
//! pool, so each move — pop an outs head, insert before a known edge, splice a
//! whole ins list in after a known edge — is O(1) pointer rewiring.

use super::graph::{EdgeId, GraphDoc, NONE, NodeId, Property};

/// Edges never cross lines, so the per-line resolution loop is one pass over
/// the document-wide node array.
pub(super) fn solve(g: &mut GraphDoc<'_, '_>) {
    for node in 0..g.nodes.len() as NodeId {
        solve_node(g, node);
    }
}

fn solve_node(g: &mut GraphDoc<'_, '_>, node: NodeId) {
    if g.nodes[node as usize].ins_head == NONE || g.nodes[node as usize].outs_head == NONE {
        return;
    }

    // The incoming edge whose source is leftmost (first on ties: forward
    // iteration only replaces on strictly smaller sources).
    let mut ins_first = NONE;
    let mut best_src = NodeId::MAX;
    let mut e = g.nodes[node as usize].ins_head;
    while e != NONE {
        let src = g.edges[e as usize].source;
        if src < best_src {
            best_src = src;
            ins_first = e;
        }
        e = g.edges[e as usize].next_in;
    }

    // Walk this node's outgoing edges, moving each leading seq edge out of
    // the way, until the first grp edge (or the end). A moved seq edge is
    // re-sourced onto the working edge's source, inserted immediately
    // before it, and becomes the working edge itself — so successive seq
    // edges stack up in front of `ins_first`.
    let mut edge = ins_first;
    let grp = loop {
        let curr = g.nodes[node as usize].outs_head;
        if curr == NONE {
            break NONE;
        }
        match g.edges[curr as usize].prop {
            Property::Grp => break curr,
            Property::Seq => {
                pop_out_head(g, node);
                let src = g.edges[edge as usize].source;
                insert_out_before(g, src, curr, edge);
                g.edges[curr as usize].source = src;
                edge = curr;
            }
        }
    };

    // Hand this node's whole incoming list forward past the grp edge:
    // retarget every incoming edge to the grp's target and splice the list
    // immediately after the grp edge in that target's ins list.
    if grp != NONE {
        let head = g.nodes[node as usize].ins_head;
        let tail = g.nodes[node as usize].ins_tail;
        let len = g.nodes[node as usize].ins_len;
        g.nodes[node as usize].ins_head = NONE;
        g.nodes[node as usize].ins_tail = NONE;
        g.nodes[node as usize].ins_len = 0;

        let target = g.edges[grp as usize].target;
        let mut e = head;
        while e != NONE {
            g.edges[e as usize].target = target;
            e = g.edges[e as usize].next_in;
        }

        let after = g.edges[grp as usize].next_in;
        g.edges[grp as usize].next_in = head;
        g.edges[tail as usize].next_in = after;
        if after == NONE {
            g.nodes[target as usize].ins_tail = tail;
        }
        g.nodes[target as usize].ins_len += len;
    }
}

/// Detaches the head edge of `node`'s outs list.
fn pop_out_head(g: &mut GraphDoc<'_, '_>, node: NodeId) {
    let head = g.nodes[node as usize].outs_head;
    let next = g.edges[head as usize].next_out;
    g.nodes[node as usize].outs_head = next;
    if next == NONE {
        g.nodes[node as usize].outs_tail = NONE;
    } else {
        g.edges[next as usize].prev_out = NONE;
    }
    g.edges[head as usize].next_out = NONE;
}

/// Inserts `new` immediately before `before` in `src`'s outs list.
fn insert_out_before(g: &mut GraphDoc<'_, '_>, src: NodeId, new: EdgeId, before: EdgeId) {
    let prev = g.edges[before as usize].prev_out;
    g.edges[new as usize].prev_out = prev;
    g.edges[new as usize].next_out = before;
    g.edges[before as usize].prev_out = new;
    if prev == NONE {
        g.nodes[src as usize].outs_head = new;
    } else {
        g.edges[prev as usize].next_out = new;
    }
}
