//! Phase 1 of structurize: FixedDoc → GraphDoc.
//!
//! Walks the fixed spine, assigns a node index per item, and materializes the
//! grp/seq scopes as graph edges (the scope graph that `solve` then resolves).

use super::graph::{GraphDoc, GraphEdge, GraphFix, GraphNode, GraphTerm, Property};
use crate::compiler::passes::term_chain::map_term_chain;
use crate::compiler::types::{FixedComp, FixedDoc, FixedFix, FixedItem, FixedObj};
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
    prop: Property<()>,
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

pub(super) fn graphify<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b GraphDoc<'b> {
    fn lift_stack(comp: &FixedComp) -> (Vec<Property<u64>>, bool) {
        // Linear chain of grp/seq wrappers around a leaf Comp(pad). Index 0
        // is the outermost wrapper (what used to be the list head).
        let mut props: Vec<Property<u64>> = Vec::new();
        let mut cur = comp;
        let pad = loop {
            match cur {
                FixedComp::Comp(pad) => break *pad,
                FixedComp::Grp(index, comp1) => {
                    props.push(Property::Grp(*index));
                    cur = comp1;
                }
                FixedComp::Seq(index, comp1) => {
                    props.push(Property::Seq(*index));
                    cur = comp1;
                }
            }
        };
        (props, pad)
    }
    // The open-scope property map is keyed by scope index. It is threaded
    // linearly (each update replaces the binding; no earlier version is ever
    // retained), so a plain owned `BTreeMap` mutated in place is a faithful
    // replacement for the former persistent map. `BTreeMap` also gives the
    // key-ordered `values()` iteration `transpose` depends on.
    type Graph = BTreeMap<u64, Property<(u64, Option<u64>)>>;
    fn close(to_node: u64, mut props: Graph, stack: &[Property<u64>]) -> Graph {
        // Close each open grp/seq scope on the stack by recording to_node.
        for prop in stack {
            match prop {
                Property::Grp(index) => match props[index] {
                    Property::Seq(_) => unreachable!("Invariant"),
                    Property::Grp((from_node, _to_node)) => {
                        props.insert(*index, Property::Grp((from_node, Some(to_node))));
                    }
                },
                Property::Seq(index) => match props[index] {
                    Property::Grp(_) => unreachable!("Invariant"),
                    Property::Seq((from_node, _to_node)) => {
                        props.insert(*index, Property::Seq((from_node, Some(to_node))));
                    }
                },
            }
        }
        props
    }
    fn open(from_node: u64, mut props: Graph, stack: &[Property<u64>]) -> Graph {
        // Open a fresh grp/seq scope on the stack, anchored at from_node.
        for prop in stack {
            match prop {
                Property::Grp(index) => {
                    props.insert(*index, Property::Grp((from_node, None)));
                }
                Property::Seq(index) => {
                    props.insert(*index, Property::Seq((from_node, None)));
                }
            }
        }
        props
    }
    fn update(
        node: u64,
        mut props: Graph,
        scope: &[Property<u64>],
        stack: &[Property<u64>],
    ) -> (Vec<Property<u64>>, Graph) {
        // Walk scope and stack in lockstep: matching grp/seq scopes are kept
        // (the common prefix `scope[..k]`); the first divergence closes the
        // remaining scope and opens the remaining stack.
        let mut k = 0;
        let rest_stack: &[Property<u64>] = loop {
            match (scope.get(k), stack.get(k)) {
                (_, None) => {
                    props = close(node, props, &scope[k..]);
                    break &[];
                }
                (None, _) => {
                    props = open(node, props, &stack[k..]);
                    break &stack[k..];
                }
                (Some(sp), Some(stp)) => {
                    let matched = match (sp, stp) {
                        (Property::Grp(left), Property::Grp(right)) => {
                            if left > right {
                                unreachable!("Invariant")
                            }
                            left == right
                        }
                        (Property::Seq(left), Property::Seq(right)) => {
                            if left > right {
                                unreachable!("Invariant")
                            }
                            left == right
                        }
                        _ => false,
                    };
                    if matched {
                        k += 1;
                    } else {
                        props = close(node, props, &scope[k..]);
                        props = open(node, props, &stack[k..]);
                        break &stack[k..];
                    }
                }
            }
        };
        // The new scope is the matched common prefix followed by the rest.
        let mut result = scope[..k].to_vec();
        result.extend_from_slice(rest_stack);
        (result, props)
    }
    fn transpose<'a>(
        mem: &'a Bump,
        nodes: &'a [&'a GraphNode<'a>],
        props: &[Property<(u64, Option<u64>)>],
    ) {
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
        // Materialize each closed grp/seq property as a graph edge.
        for prop in props {
            match prop {
                Property::Grp((from_index, Some(to_index))) => {
                    if from_index != to_index {
                        let from_node = nodes[*from_index as usize];
                        let to_node = nodes[*to_index as usize];
                        let curr = make_edge(mem, Property::Grp(()), from_node, to_node);
                        push_ins(curr, to_node);
                        push_outs(curr, from_node);
                    }
                }
                Property::Seq((from_index, Some(to_index))) => {
                    if from_index != to_index {
                        let from_node = nodes[*from_index as usize];
                        let to_node = nodes[*to_index as usize];
                        let curr = make_edge(mem, Property::Seq(()), from_node, to_node);
                        push_ins(curr, to_node);
                        push_outs(curr, from_node);
                    }
                }
                _ => unreachable!("Invariant"),
            }
        }
    }
    fn visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b GraphDoc<'b> {
        // Walk the linear FixedDoc spine, graphifying each line's object.
        type Line<'b> = (&'b [&'b GraphNode<'b>], &'b [bool]);
        let mut breaks: Vec<Line<'b>> = Vec::new();
        let mut cur = doc;
        loop {
            match cur {
                FixedDoc::Eod => break,
                FixedDoc::Break(obj, doc1) => {
                    let (nodes2, pads1, props1) = visit_obj(mem, obj);
                    // BTreeMap::values yields the properties in key order,
                    // which is the order transpose consumes them in.
                    let props2: Vec<Property<(u64, Option<u64>)>> =
                        props1.values().copied().collect();
                    transpose(mem, nodes2, &props2);
                    breaks.push((nodes2, pads1));
                    cur = doc1;
                }
            }
        }
        let mut gdoc: &'b GraphDoc<'b> = mem.alloc(GraphDoc::Eod);
        for &(nodes2, pads1) in breaks.iter().rev() {
            gdoc = mem.alloc(GraphDoc::Break(nodes2, pads1, gdoc));
        }
        gdoc
    }
    #[allow(clippy::type_complexity)]
    fn visit_obj<'b, 'a: 'b>(
        mem: &'b Bump,
        obj: &'a FixedObj<'a>,
    ) -> (&'b [&'b GraphNode<'b>], &'b [bool], Graph) {
        // Walk the object's item chain, assigning indices and threading the
        // scope stack and the open-scope property map.
        let mut nodes_vec: Vec<&'b GraphNode<'b>> = Vec::new();
        let mut pads_vec: Vec<bool> = Vec::new();
        let mut index: u64 = 0;
        let mut scope: Vec<Property<u64>> = Vec::new();
        let mut props: Graph = BTreeMap::new();
        let mut cur = obj;
        let final_props: Graph = loop {
            match cur {
                FixedObj::Next(item, comp, obj1) => {
                    let term1 = match item {
                        FixedItem::Term(term) => map_term_chain(mem, *term),
                        FixedItem::Fix(fix) => {
                            let (fix1, scope1, props1) = visit_fix(mem, fix, index, scope, props);
                            scope = scope1;
                            props = props1;
                            mem.alloc(GraphTerm::Fix(fix1))
                        }
                    };
                    nodes_vec.push(make_node(mem, index, term1));
                    let (stack, pad) = lift_stack(comp);
                    pads_vec.push(pad);
                    let (scope2, props2) = update(index, props, &scope, &stack);
                    scope = scope2;
                    props = props2;
                    index += 1;
                    cur = obj1;
                }
                FixedObj::Last(item) => {
                    let term1 = match item {
                        FixedItem::Term(term) => map_term_chain(mem, *term),
                        FixedItem::Fix(fix) => {
                            let (fix1, scope1, props1) = visit_fix(mem, fix, index, scope, props);
                            scope = scope1;
                            props = props1;
                            mem.alloc(GraphTerm::Fix(fix1))
                        }
                    };
                    nodes_vec.push(make_node(mem, index, term1));
                    break close(index, props, &scope);
                }
            }
        };
        (
            mem.alloc_slice_copy(&nodes_vec),
            mem.alloc_slice_copy(&pads_vec),
            final_props,
        )
    }
    fn visit_fix<'b, 'a: 'b>(
        mem: &'b Bump,
        fix: &'a FixedFix<'a>,
        index: u64,
        mut scope: Vec<Property<u64>>,
        mut props: Graph,
    ) -> (&'b GraphFix<'b>, Vec<Property<u64>>, Graph) {
        // Walk the fix chain forward, threading scope/props; rebuild the
        // GraphFix bottom-up from the recorded (term, pad) pairs.
        let mut recorded: Vec<(&'b GraphTerm<'b>, bool)> = Vec::new();
        let mut cur = fix;
        let last_term: &'b GraphTerm<'b> = loop {
            match cur {
                FixedFix::Next(term, comp, fix1) => {
                    let term1 = map_term_chain(mem, *term);
                    let (stack, pad) = lift_stack(comp);
                    let (scope1, props1) = update(index, props, &scope, &stack);
                    recorded.push((term1, pad));
                    scope = scope1;
                    props = props1;
                    cur = fix1;
                }
                FixedFix::Last(term) => break map_term_chain(mem, *term),
            }
        };
        let mut gfix: &'b GraphFix<'b> = mem.alloc(GraphFix::Last(last_term));
        for &(term1, pad) in recorded.iter().rev() {
            gfix = mem.alloc(GraphFix::Next(term1, gfix, pad));
        }
        (gfix, scope, props)
    }
    visit_doc(mem, doc)
}
