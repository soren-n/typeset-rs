//! Phase 2 of resolve_scopes: resolve the scope graph in place.
//!
//! For each node with both incoming and outgoing scope edges, the outgoing seq
//! edges are moved out of the way (re-sourced onto the incoming side) and the
//! incoming edges are handed forward past the first outgoing grp edge, leaving
//! a graph the rebuild phase can walk as a plain composition spine.

use super::graph::{EdgeId, LineGraph, Property};

pub(super) fn solve(doc: &mut super::graph::GraphDoc<'_>) {
    for line in doc {
        solve_line(line);
    }
}

/// The position of `edge` within `list` (it is always present by invariant).
fn position(list: &[EdgeId], edge: EdgeId) -> usize {
    list.iter()
        .position(|&e| e == edge)
        .expect("Invariant: edge is on its endpoint's list")
}

fn solve_line(g: &mut LineGraph) {
    for node in 0..g.nodes.len() {
        if g.nodes[node].ins.is_empty() || g.nodes[node].outs.is_empty() {
            continue;
        }

        // The incoming edge whose source is leftmost (first on ties).
        let ins_first = *g.nodes[node]
            .ins
            .iter()
            .min_by_key(|&&e| g.edges[e as usize].source)
            .expect("non-empty ins");

        // Walk this node's outgoing edges, moving each leading seq edge out of
        // the way, until the first grp edge (or the end). A moved seq edge is
        // re-sourced onto the working edge's source, inserted immediately
        // before it, and becomes the working edge itself — so successive seq
        // edges stack up in front of `ins_first`.
        let mut edge = ins_first;
        let grp = loop {
            match g.nodes[node].outs.first().copied() {
                None => break None,
                Some(curr) => match g.edges[curr as usize].prop {
                    Property::Grp => break Some(curr),
                    Property::Seq => {
                        g.nodes[node].outs.remove(0);
                        let src = g.edges[edge as usize].source;
                        let pos = position(&g.nodes[src as usize].outs, edge);
                        g.nodes[src as usize].outs.insert(pos, curr);
                        g.edges[curr as usize].source = src;
                        edge = curr;
                    }
                },
            }
        };

        // Hand this node's whole incoming list forward past the grp edge:
        // retarget every incoming edge to the grp's target and splice the list
        // immediately after the grp edge in that target's ins list.
        if let Some(grp) = grp {
            let ins = std::mem::take(&mut g.nodes[node].ins);
            let target = g.edges[grp as usize].target;
            for &e in &ins {
                g.edges[e as usize].target = target;
            }
            let pos = position(&g.nodes[target as usize].ins, grp);
            g.nodes[target as usize].ins.splice(pos + 1..pos + 1, ins);
        }
    }
}
