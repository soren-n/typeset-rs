//! Phase 2 of structurize: GraphDoc → GraphDoc.
//!
//! Resolves the grp/seq scope graph in place: for each node it moves the
//! incoming scope edges past any seq edges, leaving a graph the rebuild phase
//! can walk as a plain composition spine.

use super::graph::{GraphDoc, GraphEdge, GraphNode, Property, build_graph_doc, graph_lines};
use bumpalo::Bump;

fn move_ins<'a>(head: &'a GraphEdge<'a>, tail: &'a GraphEdge<'a>, edge: &'a GraphEdge<'a>) {
    fn remove_ins<'a>(ins: &'a GraphEdge<'a>) {
        let node = ins.target.get();
        node.ins_head.set(None);
        node.ins_tail.set(None)
    }
    fn append_ins<'a>(head: &'a GraphEdge<'a>, tail: &'a GraphEdge<'a>, edge: &'a GraphEdge<'a>) {
        fn set_targets<'a>(node: &'a GraphNode<'a>, ins: Option<&'a GraphEdge<'a>>) {
            let mut cur = ins;
            while let Some(edge) = cur {
                edge.target.set(node);
                cur = edge.ins_next.get();
            }
        }
        let node = edge.target.get();
        set_targets(node, Some(head));
        match edge.ins_next.get() {
            None => {
                edge.ins_next.set(Some(head));
                head.ins_prev.set(Some(edge));
                node.ins_tail.set(Some(tail))
            }
            Some(next) => {
                tail.ins_next.set(Some(next));
                next.ins_prev.set(Some(tail));
                edge.ins_next.set(Some(head));
                head.ins_prev.set(Some(edge))
            }
        }
    }
    remove_ins(head);
    append_ins(head, tail, edge)
}
fn move_out<'a>(curr: &'a GraphEdge<'a>, edge: &'a GraphEdge<'a>) {
    fn remove_out<'a>(curr: &'a GraphEdge<'a>) {
        let node = curr.source.get();
        match (curr.outs_prev.get(), curr.outs_next.get()) {
            (None, None) => {
                node.outs_head.set(None);
                node.outs_tail.set(None)
            }
            (Some(prev), None) => {
                curr.outs_prev.set(None);
                prev.outs_next.set(None);
                node.outs_tail.set(Some(prev))
            }
            (None, Some(next)) => {
                curr.outs_next.set(None);
                next.outs_prev.set(None);
                node.outs_head.set(Some(next))
            }
            (Some(prev), Some(next)) => {
                curr.outs_prev.set(None);
                curr.outs_next.set(None);
                prev.outs_next.set(Some(next));
                next.outs_prev.set(Some(prev))
            }
        }
    }
    fn prepend_out<'a>(curr: &'a GraphEdge<'a>, edge: &'a GraphEdge<'a>) {
        let node = edge.source.get();
        curr.source.set(node);
        match edge.outs_prev.get() {
            None => {
                curr.outs_next.set(Some(edge));
                edge.outs_prev.set(Some(curr));
                node.outs_head.set(Some(curr))
            }
            Some(prev) => {
                prev.outs_next.set(Some(curr));
                curr.outs_prev.set(Some(prev));
                curr.outs_next.set(Some(edge));
                edge.outs_prev.set(Some(curr));
            }
        }
    }
    remove_out(curr);
    prepend_out(curr, edge)
}
// Walks the outs edges from `outs`, moving each Seq edge out of the way,
// and returns the first Grp edge (or None if the edges are exhausted).
fn resolve<'a>(edge: &'a GraphEdge<'a>, outs: &'a GraphEdge<'a>) -> Option<&'a GraphEdge<'a>> {
    let mut maybe_curr = Some(outs);
    let mut edge = edge;
    loop {
        match maybe_curr {
            None => break None,
            Some(curr) => match curr.prop {
                Property::Grp => break Some(curr),
                Property::Seq => {
                    let curr1 = curr.outs_next.get();
                    move_out(curr, edge);
                    edge = curr;
                    maybe_curr = curr1;
                }
            },
        }
    }
}
fn leftmost<'a>(head: &'a GraphEdge<'a>) -> &'a GraphEdge<'a> {
    let mut curr = head;
    let mut index = head.source.get().index;
    let mut result = head;
    while let Some(next) = curr.ins_next.get() {
        let index1 = next.source.get().index;
        if index1 < index {
            index = index1;
            result = next;
        }
        curr = next;
    }
    result
}
fn visit_node<'a>(nodes: &'a [&'a GraphNode<'a>]) {
    for node in nodes {
        let ins_head = node.ins_head.get();
        let ins_tail = node.ins_tail.get();
        let outs_head = node.outs_head.get();
        let outs_tail = node.outs_tail.get();
        // Each intrusive list sets head and tail together, so a half-set list is
        // a broken invariant (asserted in every build, as the old match did).
        assert!(ins_head.is_some() == ins_tail.is_some(), "Invariant");
        assert!(outs_head.is_some() == outs_tail.is_some(), "Invariant");

        // Only nodes with both incoming and outgoing edges need solving.
        let (Some(ins_head), Some(ins_tail)) = (ins_head, ins_tail) else {
            continue;
        };
        let Some(outs_head) = outs_head else {
            continue;
        };

        let ins_first = leftmost(ins_head);
        if let Some(outs_head1) = resolve(ins_first, outs_head) {
            move_ins(ins_head, ins_tail, outs_head1);
        }
    }
}

pub(super) fn solve<'a>(mem: &'a Bump, doc: &'a GraphDoc<'a>) -> &'a GraphDoc<'a> {
    // Walk the linear spine, solving each line's graph in place, then reassemble
    // it (the line payloads are unchanged — `solve` mutates the shared graph
    // that `nodes` points into rather than rebuilding it).
    let lines = graph_lines(doc);
    for &(nodes, _pads) in &lines {
        visit_node(nodes);
    }
    build_graph_doc(mem, &lines)
}
