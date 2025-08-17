//! Pass 5: FixedDoc → RebuildDoc (rebuild with graph structure)

use crate::{
    compiler::types::{
        FixedComp, FixedDoc, FixedFix, FixedItem, FixedObj, FixedTerm, GraphDoc, GraphEdge,
        GraphFix, GraphNode, GraphTerm, Property, PropertyTransformer, RebuildCont, RebuildDoc,
        RebuildFix, RebuildObj, RebuildTerm, TermTransformer, TopologyResult, U64Transformer,
    },
    list::{self as _list, List},
    map::{self as _map, Map},
    order::total,
    util::compose,
};
use bumpalo::Bump;
use std::cell::Cell;

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

// Helper function to copy graph terms between memory regions
fn copy_graph_term<'b, 'a: 'b>(mem: &'b Bump, term: &'a GraphTerm<'a>) -> &'b GraphTerm<'b> {
    match term {
        GraphTerm::Null => mem.alloc(GraphTerm::Null),
        GraphTerm::Text(data) => mem.alloc(GraphTerm::Text(data)),
        GraphTerm::Fix(fix) => {
            let fix1 = copy_graph_fix(mem, fix);
            mem.alloc(GraphTerm::Fix(fix1))
        }
        GraphTerm::Nest(term1) => {
            let term2 = copy_graph_term(mem, term1);
            mem.alloc(GraphTerm::Nest(term2))
        }
        GraphTerm::Pack(index, term1) => {
            let term2 = copy_graph_term(mem, term1);
            mem.alloc(GraphTerm::Pack(*index, term2))
        }
    }
}

// Helper function to copy graph fixes between memory regions
fn copy_graph_fix<'b, 'a: 'b>(mem: &'b Bump, fix: &'a GraphFix<'a>) -> &'b GraphFix<'b> {
    match fix {
        GraphFix::Last(term) => {
            let term1 = copy_graph_term(mem, term);
            mem.alloc(GraphFix::Last(term1))
        }
        GraphFix::Next(term, fix1, pad) => {
            let term1 = copy_graph_term(mem, term);
            let fix2 = copy_graph_fix(mem, fix1);
            mem.alloc(GraphFix::Next(term1, fix2, *pad))
        }
    }
}

pub fn structurize<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b RebuildDoc<'b> {
    fn _eod<'a>(mem: &'a Bump) -> &'a GraphDoc<'a> {
        mem.alloc(GraphDoc::Eod)
    }
    fn _break<'a>(
        mem: &'a Bump,
        nodes: &'a List<'a, &'a GraphNode<'a>>,
        pads: &'a List<'a, bool>,
        doc: &'a GraphDoc<'a>,
    ) -> &'a GraphDoc<'a> {
        mem.alloc(GraphDoc::Break(nodes, pads, doc))
    }
    fn _null<'a>(mem: &'a Bump) -> &'a GraphTerm<'a> {
        mem.alloc(GraphTerm::Null)
    }
    fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a GraphTerm<'a> {
        mem.alloc(GraphTerm::Text(data))
    }
    fn _fix<'a>(mem: &'a Bump, fix: &'a GraphFix<'a>) -> &'a GraphTerm<'a> {
        mem.alloc(GraphTerm::Fix(fix))
    }
    fn _nest<'a>(mem: &'a Bump, term: &'a GraphTerm<'a>) -> &'a GraphTerm<'a> {
        mem.alloc(GraphTerm::Nest(term))
    }
    fn _pack<'a>(mem: &'a Bump, index: u64, term: &'a GraphTerm<'a>) -> &'a GraphTerm<'a> {
        mem.alloc(GraphTerm::Pack(index, term))
    }
    fn _fix_last<'a>(mem: &'a Bump, term: &'a GraphTerm<'a>) -> &'a GraphFix<'a> {
        mem.alloc(GraphFix::Last(term))
    }
    fn _fix_next<'a>(
        mem: &'a Bump,
        left: &'a GraphTerm<'a>,
        right: &'a GraphFix<'a>,
        pad: bool,
    ) -> &'a GraphFix<'a> {
        mem.alloc(GraphFix::Next(left, right, pad))
    }
    fn _unit_grp() -> Property<()> {
        Property::Grp(())
    }
    fn _unit_seq() -> Property<()> {
        Property::Seq(())
    }
    fn _unary_grp(index: u64) -> Property<u64> {
        Property::Grp(index)
    }
    fn _unary_seq(index: u64) -> Property<u64> {
        Property::Seq(index)
    }
    fn _binary_grp(from_index: u64, to_index: Option<u64>) -> Property<(u64, Option<u64>)> {
        Property::Grp((from_index, to_index))
    }
    fn _binary_seq(from_index: u64, to_index: Option<u64>) -> Property<(u64, Option<u64>)> {
        Property::Seq((from_index, to_index))
    }
    fn _graphify<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b GraphDoc<'b> {
        fn _lift_stack<'b, 'a: 'b>(
            mem: &'b Bump,
            comp: &'a FixedComp<'a>,
        ) -> (&'b List<'b, Property<u64>>, bool) {
            match comp {
                FixedComp::Comp(pad) => (_list::nil(mem), *pad),
                FixedComp::Grp(index, comp1) => {
                    let (props, pad) = _lift_stack(mem, comp1);
                    (_list::cons(mem, _unary_grp(*index), props), pad)
                }
                FixedComp::Seq(index, comp1) => {
                    let (props, pad) = _lift_stack(mem, comp1);
                    (_list::cons(mem, _unary_seq(*index), props), pad)
                }
            }
        }
        type Graph<'a> = Map<'a, u64, Property<(u64, Option<u64>)>>;
        fn _close<'b, 'a: 'b>(
            mem: &'b Bump,
            to_node: u64,
            props: &'a Graph<'a>,
            stack: &'a List<'a, Property<u64>>,
        ) -> &'b Graph<'b> {
            match stack {
                List::Nil => props,
                List::Cons(_, Property::Grp(index), stack1) => {
                    match props.lookup_unsafe(&total, *index) {
                        Property::Seq(_) => unreachable!("Invariant"),
                        Property::Grp((from_node, _to_node)) => {
                            let prop1 = _binary_grp(from_node, Some(to_node));
                            let props1 = props.insert(mem, &total, *index, prop1);
                            _close(mem, to_node, props1, stack1)
                        }
                    }
                }
                List::Cons(_, Property::Seq(index), stack1) => {
                    match props.lookup_unsafe(&total, *index) {
                        Property::Grp(_) => unreachable!("Invariant"),
                        Property::Seq((from_node, _to_node)) => {
                            let prop1 = _binary_seq(from_node, Some(to_node));
                            let props1 = props.insert(mem, &total, *index, prop1);
                            _close(mem, to_node, props1, stack1)
                        }
                    }
                }
            }
        }
        fn _open<'b, 'a: 'b>(
            mem: &'b Bump,
            from_node: u64,
            props: &'a Graph<'a>,
            stack: &'a List<Property<u64>>,
        ) -> &'b Graph<'b> {
            match stack {
                List::Nil => props,
                List::Cons(_, Property::Grp(index), stack1) => {
                    let prop1 = _binary_grp(from_node, None);
                    let props1 = props.insert(mem, &total, *index, prop1);
                    _open(mem, from_node, props1, stack1)
                }
                List::Cons(_, Property::Seq(index), stack1) => {
                    let prop1 = _binary_seq(from_node, None);
                    let props1 = props.insert(mem, &total, *index, prop1);
                    _open(mem, from_node, props1, stack1)
                }
            }
        }
        fn _update<'b, 'a: 'b>(
            mem: &'b Bump,
            node: u64,
            props: &'a Graph<'a>,
            scope: &'b List<'b, Property<u64>>,
            stack: &'b List<'b, Property<u64>>,
        ) -> (&'b List<'b, Property<u64>>, &'b Graph<'b>) {
            match (scope, stack) {
                (_, List::Nil) => {
                    let props1 = _close(mem, node, props, scope);
                    (_list::nil(mem), props1)
                }
                (List::Nil, _) => {
                    let props1 = _open(mem, node, props, stack);
                    (stack, props1)
                }
                (
                    List::Cons(_, Property::Grp(left), scope1),
                    List::Cons(_, Property::Grp(right), stack1),
                ) => {
                    if left > right {
                        unreachable!("Invariant")
                    }
                    if left == right {
                        let (stack2, props1) = _update(mem, node, props, scope1, stack1);
                        (_list::cons(mem, _unary_grp(*left), stack2), props1)
                    } else {
                        let props1 = _close(mem, node, props, scope);
                        let props2 = _open(mem, node, props1, stack);
                        (stack, props2)
                    }
                }
                (
                    List::Cons(_, Property::Seq(left), scope1),
                    List::Cons(_, Property::Seq(right), stack1),
                ) => {
                    if left > right {
                        unreachable!("Invariant")
                    }
                    if left == right {
                        let (stack2, props1) = _update(mem, node, props, scope1, stack1);
                        (_list::cons(mem, _unary_seq(*left), stack2), props1)
                    } else {
                        let props1 = _close(mem, node, props, scope);
                        let props2 = _open(mem, node, props1, stack);
                        (stack, props2)
                    }
                }
                _ => {
                    let props1 = _close(mem, node, props, scope);
                    let props2 = _open(mem, node, props1, stack);
                    (stack, props2)
                }
            }
        }
        fn _transpose<'a>(
            mem: &'a Bump,
            nodes: &'a List<'a, &'a GraphNode<'a>>,
            props: &'a List<'a, Property<(u64, Option<u64>)>>,
        ) {
            fn _push_ins<'a>(edge: &'a GraphEdge<'a>, node: &'a GraphNode<'a>) {
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
            fn _push_outs<'a>(edge: &'a GraphEdge<'a>, node: &'a GraphNode<'a>) {
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
            match props {
                List::Nil => (),
                List::Cons(_, Property::Grp((from_index, Some(to_index))), props1) => {
                    if from_index == to_index {
                        _transpose(mem, nodes, props1)
                    } else {
                        let from_node = nodes.get_unsafe(*from_index);
                        let to_node = nodes.get_unsafe(*to_index);
                        let curr = make_edge(mem, _unit_grp(), from_node, to_node);
                        _push_ins(curr, to_node);
                        _push_outs(curr, from_node);
                        _transpose(mem, nodes, props1)
                    }
                }
                List::Cons(_, Property::Seq((from_index, Some(to_index))), props1) => {
                    if from_index == to_index {
                        _transpose(mem, nodes, props1)
                    } else {
                        let from_node = nodes.get_unsafe(*from_index);
                        let to_node = nodes.get_unsafe(*to_index);
                        let curr = make_edge(mem, _unit_seq(), from_node, to_node);
                        _push_ins(curr, to_node);
                        _push_outs(curr, from_node);
                        _transpose(mem, nodes, props1)
                    }
                }
                _ => unreachable!("Invariant"),
            }
        }
        fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b GraphDoc<'b> {
            match doc {
                FixedDoc::Eod => _eod(mem),
                FixedDoc::Break(obj, doc1) => {
                    let scope = _list::nil(mem);
                    let nodes = mem.alloc(|_mem, nodes| nodes);
                    let pads = mem.alloc(|_mem, pads| pads);
                    let props = _map::empty(mem);
                    let (nodes1, pads1, props1) =
                        _visit_obj(mem, obj, 0, scope, nodes, pads, props);
                    let nodes2 = nodes1(mem, _list::nil(mem));
                    let props2 = props1.values(mem).fold(
                        mem,
                        _list::nil(mem),
                        mem.alloc(|mem, item: Property<(u64, Option<u64>)>, items| {
                            _list::cons(mem, item, items)
                        }),
                    );
                    _transpose(mem, nodes2, props2);
                    let doc2 = _visit_doc(mem, doc1);
                    _break(mem, nodes2, pads1(mem, _list::nil(mem)), doc2)
                }
            }
        }
        fn _visit_obj<'b, 'a: 'b>(
            mem: &'b Bump,
            obj: &'a FixedObj<'a>,
            index: u64,
            scope: &'a List<'a, Property<u64>>,
            nodes: &'b dyn Fn(
                &'b Bump,
                &'b List<'b, &'b GraphNode<'b>>,
            ) -> &'b List<'b, &'b GraphNode<'b>>,
            pads: &'b dyn Fn(&'b Bump, &'b List<'b, bool>) -> &'b List<'b, bool>,
            props: &'a Graph<'a>,
        ) -> (
            &'b dyn Fn(
                &'b Bump,
                &'b List<'b, &'b GraphNode<'b>>,
            ) -> &'b List<'b, &'b GraphNode<'b>>,
            &'b dyn Fn(&'b Bump, &'b List<'b, bool>) -> &'b List<'b, bool>,
            &'b Graph<'b>,
        ) {
            match obj {
                FixedObj::Next(term, comp, obj1) => match term {
                    FixedItem::Term(term) => _visit_term(
                        mem,
                        term,
                        mem.alloc(move |mem, term1| {
                            let nodes2 = compose(
                                mem,
                                nodes,
                                mem.alloc(move |mem, nodes1| {
                                    _list::cons(mem, make_node(mem, index, term1), nodes1)
                                }),
                            );
                            let (stack, pad) = _lift_stack(mem, comp);
                            let pads2 = compose(
                                mem,
                                pads,
                                mem.alloc(move |mem, pads1| _list::cons(mem, pad, pads1)),
                            );
                            let (scope1, props1) = _update(mem, index, props, scope, stack);
                            _visit_obj(mem, obj1, index + 1, scope1, nodes2, pads2, props1)
                        }),
                    ),
                    FixedItem::Fix(fix) => {
                        let (fix1, scope1, props1) = _visit_fix(mem, fix, index, scope, props);
                        let nodes2 = compose(
                            mem,
                            nodes,
                            mem.alloc(move |mem, nodes1| {
                                _list::cons(mem, make_node(mem, index, _fix(mem, fix1)), nodes1)
                            }),
                        );
                        let (stack, pad) = _lift_stack(mem, comp);
                        let pads2 = compose(
                            mem,
                            pads,
                            mem.alloc(move |mem, pads1| _list::cons(mem, pad, pads1)),
                        );
                        let (scope2, props2) = _update(mem, index, props1, scope1, stack);
                        _visit_obj(mem, obj1, index + 1, scope2, nodes2, pads2, props2)
                    }
                },
                FixedObj::Last(term) => match term {
                    FixedItem::Term(term) => _visit_term(
                        mem,
                        term,
                        mem.alloc(move |mem, term1| {
                            let nodes2 = compose(
                                mem,
                                nodes,
                                mem.alloc(move |mem, nodes1| {
                                    _list::cons(mem, make_node(mem, index, term1), nodes1)
                                }),
                            );
                            let props1 = _close(mem, index, props, scope);
                            (nodes2, pads, props1)
                        }),
                    ),
                    FixedItem::Fix(fix) => {
                        let (fix1, scope1, props1) = _visit_fix(mem, fix, index, scope, props);
                        let nodes2 = compose(
                            mem,
                            nodes,
                            mem.alloc(move |mem, nodes1| {
                                _list::cons(mem, make_node(mem, index, _fix(mem, fix1)), nodes1)
                            }),
                        );
                        let props2 = _close(mem, index, props1, scope1);
                        (nodes2, pads, props2)
                    }
                },
            }
        }
        fn _visit_term<'b, 'a: 'b, R>(
            mem: &'b Bump,
            term: &'a FixedTerm<'a>,
            cont: &'b dyn Fn(&'b Bump, &'b GraphTerm<'b>) -> R,
        ) -> R {
            match term {
                FixedTerm::Null => cont(mem, _null(mem)),
                FixedTerm::Text(data) => cont(mem, _text(mem, data)),
                FixedTerm::Nest(term1) => _visit_term(
                    mem,
                    term1,
                    compose(mem, cont, mem.alloc(|mem, term2| _nest(mem, term2))),
                ),
                FixedTerm::Pack(index, term1) => _visit_term(
                    mem,
                    term1,
                    compose(mem, cont, mem.alloc(|mem, term2| _pack(mem, *index, term2))),
                ),
            }
        }
        fn _visit_fix<'b, 'a: 'b>(
            mem: &'b Bump,
            fix: &'a FixedFix<'a>,
            index: u64,
            scope: &'a List<'a, Property<u64>>,
            props: &'a Graph<'a>,
        ) -> (&'b GraphFix<'b>, &'b List<'b, Property<u64>>, &'b Graph<'b>) {
            match fix {
                FixedFix::Next(term, comp, fix1) => _visit_term(
                    mem,
                    term,
                    mem.alloc(move |mem, term1| {
                        let (stack, pad) = _lift_stack(mem, comp);
                        let (scope1, props1) = _update(mem, index, props, scope, stack);
                        let (fix2, scope2, props2) = _visit_fix(mem, fix1, index, scope1, props1);
                        (_fix_next(mem, term1, fix2, pad), scope2, props2)
                    }),
                ),
                FixedFix::Last(term) => _visit_term(
                    mem,
                    term,
                    mem.alloc(move |mem, term1| (_fix_last(mem, term1), scope, props)),
                ),
            }
        }
        _visit_doc(mem, doc)
    }
    fn _solve<'a>(mem: &'a Bump, doc: &'a GraphDoc<'a>) -> &'a GraphDoc<'a> {
        fn _move_ins<'a>(
            head: &'a GraphEdge<'a>,
            tail: &'a GraphEdge<'a>,
            edge: &'a GraphEdge<'a>,
        ) {
            fn _remove_ins<'a>(ins: &'a GraphEdge<'a>) {
                let node = ins.target.get();
                node.ins_head.set(None);
                node.ins_tail.set(None)
            }
            fn _append_ins<'a>(
                head: &'a GraphEdge<'a>,
                tail: &'a GraphEdge<'a>,
                edge: &'a GraphEdge<'a>,
            ) {
                fn _set_targets<'a>(node: &'a GraphNode<'a>, ins: Option<&'a GraphEdge<'a>>) {
                    match ins {
                        None => (),
                        Some(edge) => {
                            edge.target.set(node);
                            _set_targets(node, edge.ins_next.get())
                        }
                    }
                }
                let node = edge.target.get();
                _set_targets(node, Some(head));
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
            _remove_ins(head);
            _append_ins(head, tail, edge)
        }
        fn _move_out<'a>(curr: &'a GraphEdge<'a>, edge: &'a GraphEdge<'a>) {
            fn _remove_out<'a>(curr: &'a GraphEdge<'a>) {
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
            fn _prepend_out<'a>(curr: &'a GraphEdge<'a>, edge: &'a GraphEdge<'a>) {
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
            _remove_out(curr);
            _prepend_out(curr, edge)
        }
        fn _resolve<'a, R>(
            mem: &'a Bump,
            edge: &'a GraphEdge<'a>,
            outs: &'a GraphEdge<'a>,
            none: &'a dyn Fn(&'a Bump) -> R,
            some: &'a dyn Fn(&'a Bump, &'a GraphEdge<'a>) -> R,
        ) -> R {
            fn _visit<'a, R>(
                mem: &'a Bump,
                maybe_curr: Option<&'a GraphEdge<'a>>,
                edge: &'a GraphEdge<'a>,
                none: &'a dyn Fn(&'a Bump) -> R,
                some: &'a dyn Fn(&'a Bump, &'a GraphEdge<'a>) -> R,
            ) -> R {
                match maybe_curr {
                    None => none(mem),
                    Some(curr) => match curr.prop {
                        Property::Grp(()) => some(mem, curr),
                        Property::Seq(()) => {
                            let curr1 = curr.outs_next.get();
                            _move_out(curr, edge);
                            _visit(mem, curr1, curr, none, some)
                        }
                    },
                }
            }
            _visit(mem, Some(outs), edge, none, some)
        }
        fn _leftmost<'a>(mem: &'a Bump, head: &'a GraphEdge<'a>) -> &'a GraphEdge<'a> {
            fn _visit<'a>(
                mem: &'a Bump,
                curr: &'a GraphEdge<'a>,
                index: u64,
                result: &'a GraphEdge<'a>,
            ) -> &'a GraphEdge<'a> {
                match curr.ins_next.get() {
                    None => result,
                    Some(next) => {
                        let index1 = next.source.get().index;
                        if index1 < index {
                            _visit(mem, next, index1, next)
                        } else {
                            _visit(mem, next, index, result)
                        }
                    }
                }
            }
            _visit(mem, head, head.source.get().index, head)
        }
        fn _visit_doc<'a>(mem: &'a Bump, doc: &'a GraphDoc<'a>) -> &'a GraphDoc<'a> {
            match doc {
                GraphDoc::Eod => _eod(mem),
                GraphDoc::Break(nodes, pads, doc1) => {
                    let count = nodes.length();
                    _visit_node(mem, count, 0, nodes);
                    let doc2 = _visit_doc(mem, doc1);
                    _break(mem, nodes, pads, doc2)
                }
            }
        }
        fn _visit_node<'a>(
            mem: &'a Bump,
            count: u64,
            index: u64,
            nodes: &'a List<'a, &'a GraphNode<'a>>,
        ) {
            if count == index {
                return;
            }
            let node = nodes.get_unsafe(index);
            match (
                (node.ins_head.get(), node.ins_tail.get()),
                (node.outs_head.get(), node.outs_tail.get()),
            ) {
                ((Some(ins_head), Some(ins_tail)), (Some(outs_head), Some(_outs_tail))) => {
                    let ins_first = _leftmost(mem, ins_head);
                    _resolve(
                        mem,
                        ins_first,
                        outs_head,
                        mem.alloc(move |mem| _visit_node(mem, count, index + 1, nodes)),
                        mem.alloc(move |mem, outs_head1| {
                            _move_ins(ins_head, ins_tail, outs_head1);
                            _visit_node(mem, count, index + 1, nodes)
                        }),
                    )
                }
                ((Some(_), None), _)
                | ((None, Some(_)), _)
                | (_, (Some(_), None))
                | (_, (None, Some(_))) => unreachable!("Invariant"),
                (_, _) => _visit_node(mem, count, index + 1, nodes),
            }
        }
        _visit_doc(mem, doc)
    }
    fn _rebuild<'b, 'a: 'b>(mem: &'b Bump, doc: &'a GraphDoc<'a>) -> &'b RebuildDoc<'b> {
        fn _eod<'a>(mem: &'a Bump) -> &'a RebuildDoc<'a> {
            mem.alloc(RebuildDoc::Eod)
        }
        fn _break<'a>(
            mem: &'a Bump,
            obj: &'a RebuildObj<'a>,
            doc: &'a RebuildDoc<'a>,
        ) -> &'a RebuildDoc<'a> {
            mem.alloc(RebuildDoc::Break(obj, doc))
        }
        fn _term<'a>(mem: &'a Bump, term: &'a RebuildTerm<'a>) -> &'a RebuildObj<'a> {
            mem.alloc(RebuildObj::Term(term))
        }
        fn _fix<'a>(mem: &'a Bump, fix: &'a RebuildFix<'a>) -> &'a RebuildObj<'a> {
            mem.alloc(RebuildObj::Fix(fix))
        }
        fn _grp<'a>(mem: &'a Bump, obj: &'a RebuildObj<'a>) -> &'a RebuildObj<'a> {
            mem.alloc(RebuildObj::Grp(obj))
        }
        fn _seq<'a>(mem: &'a Bump, obj: &'a RebuildObj<'a>) -> &'a RebuildObj<'a> {
            mem.alloc(RebuildObj::Seq(obj))
        }
        fn _comp<'a>(
            mem: &'a Bump,
            left: &'a RebuildObj<'a>,
            right: &'a RebuildObj<'a>,
            pad: bool,
        ) -> &'a RebuildObj<'a> {
            mem.alloc(RebuildObj::Comp(left, right, pad))
        }
        fn _fix_term<'a>(mem: &'a Bump, term: &'a RebuildTerm<'a>) -> &'a RebuildFix<'a> {
            mem.alloc(RebuildFix::Term(term))
        }
        fn _fix_comp<'a>(
            mem: &'a Bump,
            left: &'a RebuildFix<'a>,
            right: &'a RebuildFix<'a>,
            pad: bool,
        ) -> &'a RebuildFix<'a> {
            mem.alloc(RebuildFix::Comp(left, right, pad))
        }
        fn _null<'a>(mem: &'a Bump) -> &'a RebuildTerm<'a> {
            mem.alloc(RebuildTerm::Null)
        }
        fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a RebuildTerm<'a> {
            mem.alloc(RebuildTerm::Text(data))
        }
        fn _nest<'a>(mem: &'a Bump, term: &'a RebuildTerm<'a>) -> &'a RebuildTerm<'a> {
            mem.alloc(RebuildTerm::Nest(term))
        }
        fn _pack<'a>(mem: &'a Bump, index: u64, term: &'a RebuildTerm<'a>) -> &'a RebuildTerm<'a> {
            mem.alloc(RebuildTerm::Pack(index, term))
        }
        fn __comp<'a>(
            mem: &'a Bump,
            left: &'a RebuildObj<'a>,
            pad: bool,
            right: &'a RebuildObj<'a>,
        ) -> &'a RebuildObj<'a> {
            _comp(mem, left, right, pad)
        }
        fn _topology<'b, 'a: 'b>(
            mem: &'b Bump,
            nodes: &'a List<'a, &'a GraphNode<'a>>,
        ) -> TopologyResult<'b> {
            fn _num_ins<'a>(node: &'a GraphNode<'a>) -> u64 {
                fn _visit<'a>(maybe_edge: Option<&'a GraphEdge<'a>>, num: u64) -> u64 {
                    match maybe_edge {
                        None => num,
                        Some(edge) => _visit(edge.ins_next.get(), num + 1),
                    }
                }
                _visit(node.ins_head.get(), 0)
            }
            fn _prop_outs<'b, 'a: 'b>(
                mem: &'b Bump,
                node: &'a GraphNode<'a>,
            ) -> &'b List<'b, Property<()>> {
                fn _visit<'b, 'a: 'b>(
                    mem: &'b Bump,
                    maybe_edge: Option<&'a GraphEdge<'a>>,
                    props: &'b dyn Fn(
                        &'b Bump,
                        &'b List<'b, Property<()>>,
                    ) -> &'b List<'b, Property<()>>,
                ) -> &'b List<'b, Property<()>> {
                    match maybe_edge {
                        None => props(mem, _list::nil(mem)),
                        Some(edge) => _visit(
                            mem,
                            edge.outs_next.get(),
                            compose(
                                mem,
                                props,
                                mem.alloc(|mem, props1| _list::cons(mem, edge.prop, props1)),
                            ),
                        ),
                    }
                }
                _visit(mem, node.outs_head.get(), mem.alloc(|_mem, props| props))
            }
            fn _visit<'b, 'a: 'b>(
                mem: &'b Bump,
                nodes: &'a List<'a, &'a GraphNode<'a>>,
                index: u64,
                terms: TermTransformer<'b>,
                ins: U64Transformer<'b>,
                outs: PropertyTransformer<'b>,
            ) -> TopologyResult<'b> {
                if index == nodes.length() {
                    (
                        terms(mem, _list::nil(mem)),
                        ins(mem, _list::nil(mem)),
                        outs(mem, _list::nil(mem)),
                    )
                } else {
                    let node = nodes.get_unsafe(index);
                    let term1 = copy_graph_term(mem, node.term);
                    let num_ins = _num_ins(node);
                    let prop_outs = _prop_outs(mem, node);
                    _visit(
                        mem,
                        nodes,
                        index + 1,
                        compose(
                            mem,
                            terms,
                            mem.alloc(move |mem, term2| _list::cons(mem, term1, term2)),
                        ),
                        compose(
                            mem,
                            ins,
                            mem.alloc(move |mem, ins1| _list::cons(mem, num_ins, ins1)),
                        ),
                        compose(
                            mem,
                            outs,
                            mem.alloc(move |mem, outs1| _list::cons(mem, prop_outs, outs1)),
                        ),
                    )
                }
            }
            _visit(
                mem,
                nodes,
                0,
                mem.alloc(|_mem, terms| terms),
                mem.alloc(|_mem, ins| ins),
                mem.alloc(|_mem, outs| outs),
            )
        }
        fn _open<'a>(
            mem: &'a Bump,
            props: &'a List<'a, Property<()>>,
            stack: &'a List<'a, RebuildCont<'a>>,
            partial: &'a dyn Fn(&'a Bump, &'a RebuildObj<'a>) -> &'a RebuildObj<'a>,
        ) -> &'a List<'a, RebuildCont<'a>> {
            fn _visit<'a>(
                mem: &'a Bump,
                props: &'a List<'a, Property<()>>,
                stack: &'a List<'a, RebuildCont<'a>>,
            ) -> &'a List<'a, RebuildCont<'a>> {
                match props {
                    List::Nil => stack,
                    List::Cons(_, Property::Grp(()), props1) => _visit(
                        mem,
                        props1,
                        _list::cons(
                            mem,
                            RebuildCont(mem.alloc(|mem, obj| _grp(mem, obj))),
                            stack,
                        ),
                    ),
                    List::Cons(_, Property::Seq(()), props1) => _visit(
                        mem,
                        props1,
                        _list::cons(
                            mem,
                            RebuildCont(mem.alloc(|mem, obj| _seq(mem, obj))),
                            stack,
                        ),
                    ),
                }
            }
            match stack {
                List::Cons(_, top, stack1) => _visit(
                    mem,
                    props,
                    _list::cons(
                        mem,
                        RebuildCont(mem.alloc(|mem, obj| top.0(mem, partial(mem, obj)))),
                        stack1,
                    ),
                ),
                _ => unreachable!("Invariant"),
            }
        }
        fn _close<'a>(
            mem: &'a Bump,
            count: u64,
            stack: &'a List<'a, RebuildCont<'a>>,
            term: &'a RebuildObj<'a>,
        ) -> (&'a List<'a, RebuildCont<'a>>, &'a RebuildObj<'a>) {
            fn _visit<'a>(
                mem: &'a Bump,
                count: u64,
                stack: &'a List<'a, RebuildCont<'a>>,
                result: &'a RebuildObj<'a>,
            ) -> (&'a List<'a, RebuildCont<'a>>, &'a RebuildObj<'a>) {
                if count == 0 {
                    (stack, result)
                } else {
                    match stack {
                        List::Cons(_, top, stack1) => {
                            _visit(mem, count - 1, stack1, top.0(mem, result))
                        }
                        _ => unreachable!("Invariant"),
                    }
                }
            }
            _visit(mem, count, stack, term)
        }
        fn _final<'a>(
            mem: &'a Bump,
            stack: &'a List<'a, RebuildCont<'a>>,
            term: &'a RebuildObj<'a>,
        ) -> &'a RebuildObj<'a> {
            match stack {
                List::Cons(_, last, List::Nil) => last.0(mem, term),
                _ => unreachable!("Invariant"),
            }
        }
        fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a GraphDoc<'a>) -> &'b RebuildDoc<'b> {
            match doc {
                GraphDoc::Eod => _eod(mem),
                GraphDoc::Break(nodes, pads, doc1) => {
                    let (terms, ins, outs) = _topology(mem, nodes);
                    let stack: &'b List<'b, RebuildCont<'b>> = _list::cons(
                        mem,
                        RebuildCont(mem.alloc(|_mem, obj| obj)),
                        _list::nil(mem),
                    );
                    let partial = mem.alloc(|_mem, obj| obj);
                    let obj = _visit_line(mem, terms, pads, ins, outs, stack, partial);
                    let doc2 = _visit_doc(mem, doc1);
                    _break(mem, obj, doc2)
                }
            }
        }
        fn _visit_line<'a>(
            mem: &'a Bump,
            terms: &'a List<'a, &'a GraphTerm<'a>>,
            pads: &'a List<'a, bool>,
            ins: &'a List<'a, u64>,
            outs: &'a List<'a, &'a List<'a, Property<()>>>,
            stack: &'a List<'a, RebuildCont<'a>>,
            partial: &'a dyn Fn(&'a Bump, &'a RebuildObj<'a>) -> &'a RebuildObj<'a>,
        ) -> &'a RebuildObj<'a> {
            match (terms, pads) {
                (List::Cons(_, GraphTerm::Fix(fix), List::Nil), List::Nil) => _visit_fix(
                    mem,
                    fix,
                    mem.alloc(move |mem, fix1| match (ins, outs) {
                        (List::Cons(_, 0, List::Nil), List::Cons(_, List::Nil, List::Nil)) => {
                            _final(mem, stack, partial(mem, _fix(mem, fix1)))
                        }
                        (
                            List::Cons(_, in_props, List::Nil),
                            List::Cons(_, List::Nil, List::Nil),
                        ) => {
                            let (stack1, fix2) =
                                _close(mem, *in_props, stack, partial(mem, _fix(mem, fix1)));
                            _final(mem, stack1, fix2)
                        }
                        (_, _) => unreachable!("Invariant"),
                    }),
                ),
                (List::Cons(_, term, List::Nil), List::Nil) => _visit_term(
                    mem,
                    term,
                    mem.alloc(move |mem, term1| match (ins, outs) {
                        (List::Cons(_, 0, List::Nil), List::Cons(_, List::Nil, List::Nil)) => {
                            _final(mem, stack, partial(mem, _term(mem, term1)))
                        }
                        (
                            List::Cons(_, in_props, List::Nil),
                            List::Cons(_, List::Nil, List::Nil),
                        ) => {
                            let (stack1, term2) =
                                _close(mem, *in_props, stack, partial(mem, _term(mem, term1)));
                            _final(mem, stack1, term2)
                        }
                        (_, _) => unreachable!("Invariant"),
                    }),
                ),
                (List::Cons(_, GraphTerm::Fix(fix), terms1), List::Cons(_, pad, pads1)) => {
                    _visit_fix(
                        mem,
                        fix,
                        mem.alloc(move |mem, fix1| match (ins, outs) {
                            (List::Cons(_, 0, ins1), List::Cons(_, List::Nil, outs1)) => {
                                let partial1 = compose(
                                    mem,
                                    partial,
                                    mem.alloc(move |mem, obj| {
                                        __comp(mem, _fix(mem, fix1), *pad, obj)
                                    }),
                                );
                                _visit_line(mem, terms1, pads1, ins1, outs1, stack, partial1)
                            }
                            (List::Cons(_, in_props, ins1), List::Cons(_, List::Nil, outs1)) => {
                                let (stack1, fix2) =
                                    _close(mem, *in_props, stack, partial(mem, _fix(mem, fix1)));
                                let partial1 =
                                    mem.alloc(move |mem, obj| __comp(mem, fix2, *pad, obj));
                                _visit_line(mem, terms1, pads1, ins1, outs1, stack1, partial1)
                            }
                            (List::Cons(_, 0, ins1), List::Cons(_, out_props, outs1)) => {
                                let stack1 = _open(mem, out_props, stack, partial);
                                let partial1 =
                                    mem.alloc(|mem, obj| __comp(mem, _fix(mem, fix1), *pad, obj));
                                _visit_line(mem, terms1, pads1, ins1, outs1, stack1, partial1)
                            }
                            (_, _) => unreachable!("Invariant"),
                        }),
                    )
                }
                (List::Cons(_, term, terms1), List::Cons(_, pad, pads1)) => _visit_term(
                    mem,
                    term,
                    mem.alloc(move |mem, term1| match (ins, outs) {
                        (List::Cons(_, 0, ins1), List::Cons(_, List::Nil, outs1)) => {
                            let partial1 = compose(
                                mem,
                                partial,
                                mem.alloc(move |mem, obj| {
                                    __comp(mem, _term(mem, term1), *pad, obj)
                                }),
                            );
                            _visit_line(mem, terms1, pads1, ins1, outs1, stack, partial1)
                        }
                        (List::Cons(_, in_props, ins1), List::Cons(_, List::Nil, outs1)) => {
                            let (stack1, term2) =
                                _close(mem, *in_props, stack, partial(mem, _term(mem, term1)));
                            let partial1 = mem.alloc(move |mem, obj| __comp(mem, term2, *pad, obj));
                            _visit_line(mem, terms1, pads1, ins1, outs1, stack1, partial1)
                        }
                        (List::Cons(_, 0, ins1), List::Cons(_, out_props, outs1)) => {
                            let stack1 = _open(mem, out_props, stack, partial);
                            let partial1 =
                                mem.alloc(|mem, obj| __comp(mem, _term(mem, term1), *pad, obj));
                            _visit_line(mem, terms1, pads1, ins1, outs1, stack1, partial1)
                        }
                        (_, _) => unreachable!("Invariant"),
                    }),
                ),
                (_, _) => unreachable!("Invariant"),
            }
        }
        fn _visit_term<'b, 'a: 'b, R>(
            mem: &'b Bump,
            term: &'a GraphTerm<'a>,
            cont: &'b dyn Fn(&'b Bump, &'b RebuildTerm<'b>) -> R,
        ) -> R {
            match term {
                GraphTerm::Null => cont(mem, _null(mem)),
                GraphTerm::Text(data) => cont(mem, _text(mem, data)),
                GraphTerm::Nest(term1) => _visit_term(
                    mem,
                    term1,
                    compose(mem, cont, mem.alloc(|mem, term2| _nest(mem, term2))),
                ),
                GraphTerm::Pack(index, term1) => _visit_term(
                    mem,
                    term1,
                    compose(mem, cont, mem.alloc(|mem, term2| _pack(mem, *index, term2))),
                ),
                GraphTerm::Fix(_fix) => unreachable!("Invariant"),
            }
        }
        fn _visit_fix<'b, 'a: 'b, R>(
            mem: &'b Bump,
            fix: &'a GraphFix<'a>,
            cont: &'b dyn Fn(&'b Bump, &'b RebuildFix<'b>) -> R,
        ) -> R {
            match fix {
                GraphFix::Last(term) => _visit_term(
                    mem,
                    term,
                    compose(mem, cont, mem.alloc(|mem, term1| _fix_term(mem, term1))),
                ),
                GraphFix::Next(term, fix1, pad) => _visit_term(
                    mem,
                    term,
                    mem.alloc(move |mem, term1| {
                        _visit_fix(
                            mem,
                            fix1,
                            mem.alloc(move |mem, fix2| {
                                cont(mem, _fix_comp(mem, _fix_term(mem, term1), fix2, *pad))
                            }),
                        )
                    }),
                ),
            }
        }
        _visit_doc(mem, doc)
    }
    let doc1 = _graphify(mem, doc);
    let doc2 = _solve(mem, doc1);
    _rebuild(mem, doc2)
}
