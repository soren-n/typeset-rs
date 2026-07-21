//! Pass 5: FixedDoc → RebuildDoc (rebuild with graph structure)

use crate::{
    compiler::types::{
        FixedComp, FixedDoc, FixedFix, FixedItem, FixedObj, FixedTerm, GraphDoc, GraphEdge,
        GraphFix, GraphNode, GraphTerm, Property, RebuildDoc, RebuildFix, RebuildObj, RebuildTerm,
        TopologyResult,
    },
    list::{self as _list, List},
    map::{self as _map, Map},
    order::total,
};
use bumpalo::Bump;
use std::cell::Cell;

// Builds a persistent List from a slice, with `items[0]` at the head.
fn _list_of<'a, T: Copy + Clone + std::fmt::Debug>(mem: &'a Bump, items: &[T]) -> &'a List<'a, T> {
    let mut list = _list::nil(mem);
    for item in items.iter().rev() {
        list = _list::cons(mem, *item, list);
    }
    list
}

// Defunctionalized rebuild continuations (replacing the `partial` closure and
// the RebuildCont closure stack used by `_rebuild::_visit_line`).
//
// A partial is a left composition spine: `RPartial = [(x0,p0), .. (xk,pk)]` with
// the head being the innermost (most recently added) element. Applied to an
// object it yields `Comp(x0, Comp(x1, .. Comp(xk, obj, pk) .., p1), p0)`.
type RPartial<'b> = &'b List<'b, (&'b RebuildObj<'b>, bool)>;

// A continuation step; a continuation is a list of steps applied head-first.
#[derive(Debug, Copy, Clone)]
enum RStep<'b> {
    Grp,
    Seq,
    Partial(RPartial<'b>),
}
type RCont<'b> = &'b List<'b, RStep<'b>>;
type RStack<'b> = &'b List<'b, RCont<'b>>;

// Applies a partial spine to an object (innermost first).
fn _apply_rpartial<'b>(
    mem: &'b Bump,
    partial: RPartial<'b>,
    obj: &'b RebuildObj<'b>,
) -> &'b RebuildObj<'b> {
    let mut result = obj;
    let mut cur = partial;
    while let List::Cons(_, pair, rest) = cur {
        let (left, pad) = *pair;
        result = mem.alloc(RebuildObj::Comp(left, result, pad));
        cur = rest;
    }
    result
}

// Applies a continuation (list of steps) to an object (head step first).
fn _apply_rcont<'b>(mem: &'b Bump, cont: RCont<'b>, obj: &'b RebuildObj<'b>) -> &'b RebuildObj<'b> {
    let mut result = obj;
    let mut cur = cont;
    while let List::Cons(_, step, rest) = cur {
        result = match step {
            RStep::Grp => mem.alloc(RebuildObj::Grp(result)),
            RStep::Seq => mem.alloc(RebuildObj::Seq(result)),
            RStep::Partial(partial) => _apply_rpartial(mem, partial, result),
        };
        cur = rest;
    }
    result
}

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

// A subtree still to be copied (GraphTerm and GraphFix are mutually nested).
enum ToCopy<'a> {
    Term(&'a GraphTerm<'a>),
    Fix(&'a GraphFix<'a>),
}

// A copied value ascending through the copy trampoline.
enum Copied<'b> {
    Term(&'b GraphTerm<'b>),
    Fix(&'b GraphFix<'b>),
}

// Frames for the copy trampoline.
enum CopyFrame<'b, 'a> {
    WrapNest,
    WrapPack(u64),
    WrapFixTerm,
    WrapFixLast,
    FixNextAfterTerm { fix1: &'a GraphFix<'a>, pad: bool },
    FixNextAfterFix { term: &'b GraphTerm<'b>, pad: bool },
}

// Deep-copies a graph term between memory regions. GraphTerm and GraphFix are
// mutually recursive, so this runs as a descend/ascend trampoline over an
// explicit frame stack rather than native-stack recursion.
fn copy_graph_term<'b, 'a: 'b>(mem: &'b Bump, term: &'a GraphTerm<'a>) -> &'b GraphTerm<'b> {
    let mut stack: Vec<CopyFrame<'b, 'a>> = Vec::new();
    let mut cur = ToCopy::Term(term);
    'machine: loop {
        let mut val: Copied<'b> = loop {
            match cur {
                ToCopy::Term(GraphTerm::Null) => break Copied::Term(mem.alloc(GraphTerm::Null)),
                ToCopy::Term(GraphTerm::Text(data)) => {
                    break Copied::Term(mem.alloc(GraphTerm::Text(data)));
                }
                ToCopy::Term(GraphTerm::Nest(term1)) => {
                    stack.push(CopyFrame::WrapNest);
                    cur = ToCopy::Term(term1);
                }
                ToCopy::Term(GraphTerm::Pack(index, term1)) => {
                    stack.push(CopyFrame::WrapPack(*index));
                    cur = ToCopy::Term(term1);
                }
                ToCopy::Term(GraphTerm::Fix(fix)) => {
                    stack.push(CopyFrame::WrapFixTerm);
                    cur = ToCopy::Fix(fix);
                }
                ToCopy::Fix(GraphFix::Last(term1)) => {
                    stack.push(CopyFrame::WrapFixLast);
                    cur = ToCopy::Term(term1);
                }
                ToCopy::Fix(GraphFix::Next(term1, fix1, pad)) => {
                    stack.push(CopyFrame::FixNextAfterTerm { fix1, pad: *pad });
                    cur = ToCopy::Term(term1);
                }
            }
        };
        loop {
            match stack.pop() {
                None => match val {
                    Copied::Term(term1) => return term1,
                    Copied::Fix(_) => unreachable!("Invariant"),
                },
                Some(CopyFrame::WrapNest) => match val {
                    Copied::Term(term1) => val = Copied::Term(mem.alloc(GraphTerm::Nest(term1))),
                    Copied::Fix(_) => unreachable!("Invariant"),
                },
                Some(CopyFrame::WrapPack(index)) => match val {
                    Copied::Term(term1) => {
                        val = Copied::Term(mem.alloc(GraphTerm::Pack(index, term1)));
                    }
                    Copied::Fix(_) => unreachable!("Invariant"),
                },
                Some(CopyFrame::WrapFixTerm) => match val {
                    Copied::Fix(fix1) => val = Copied::Term(mem.alloc(GraphTerm::Fix(fix1))),
                    Copied::Term(_) => unreachable!("Invariant"),
                },
                Some(CopyFrame::WrapFixLast) => match val {
                    Copied::Term(term1) => val = Copied::Fix(mem.alloc(GraphFix::Last(term1))),
                    Copied::Fix(_) => unreachable!("Invariant"),
                },
                Some(CopyFrame::FixNextAfterTerm { fix1, pad }) => match val {
                    Copied::Term(term1) => {
                        stack.push(CopyFrame::FixNextAfterFix { term: term1, pad });
                        cur = ToCopy::Fix(fix1);
                        continue 'machine;
                    }
                    Copied::Fix(_) => unreachable!("Invariant"),
                },
                Some(CopyFrame::FixNextAfterFix { term, pad }) => match val {
                    Copied::Fix(fix2) => {
                        val = Copied::Fix(mem.alloc(GraphFix::Next(term, fix2, pad)));
                    }
                    Copied::Term(_) => unreachable!("Invariant"),
                },
            }
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
            // Linear chain of grp/seq wrappers around a leaf Comp(pad).
            let mut props: Vec<Property<u64>> = Vec::new();
            let mut cur = comp;
            let pad = loop {
                match cur {
                    FixedComp::Comp(pad) => break *pad,
                    FixedComp::Grp(index, comp1) => {
                        props.push(_unary_grp(*index));
                        cur = comp1;
                    }
                    FixedComp::Seq(index, comp1) => {
                        props.push(_unary_seq(*index));
                        cur = comp1;
                    }
                }
            };
            (_list_of(mem, &props), pad)
        }
        type Graph<'a> = Map<'a, u64, Property<(u64, Option<u64>)>>;
        fn _close<'b, 'a: 'b>(
            mem: &'b Bump,
            to_node: u64,
            props: &'a Graph<'a>,
            stack: &'a List<'a, Property<u64>>,
        ) -> &'b Graph<'b> {
            // Close each open grp/seq scope on the stack by recording to_node.
            let mut props: &'b Graph<'b> = props;
            let mut cur = stack;
            loop {
                match cur {
                    List::Nil => break,
                    List::Cons(_, Property::Grp(index), stack1) => {
                        match props.lookup_unsafe(&total, *index) {
                            Property::Seq(_) => unreachable!("Invariant"),
                            Property::Grp((from_node, _to_node)) => {
                                let prop1 = _binary_grp(from_node, Some(to_node));
                                props = props.insert(mem, &total, *index, prop1);
                            }
                        }
                        cur = stack1;
                    }
                    List::Cons(_, Property::Seq(index), stack1) => {
                        match props.lookup_unsafe(&total, *index) {
                            Property::Grp(_) => unreachable!("Invariant"),
                            Property::Seq((from_node, _to_node)) => {
                                let prop1 = _binary_seq(from_node, Some(to_node));
                                props = props.insert(mem, &total, *index, prop1);
                            }
                        }
                        cur = stack1;
                    }
                }
            }
            props
        }
        fn _open<'b, 'a: 'b>(
            mem: &'b Bump,
            from_node: u64,
            props: &'a Graph<'a>,
            stack: &'a List<Property<u64>>,
        ) -> &'b Graph<'b> {
            // Open a fresh grp/seq scope on the stack, anchored at from_node.
            let mut props: &'b Graph<'b> = props;
            let mut cur = stack;
            loop {
                match cur {
                    List::Nil => break,
                    List::Cons(_, Property::Grp(index), stack1) => {
                        let prop1 = _binary_grp(from_node, None);
                        props = props.insert(mem, &total, *index, prop1);
                        cur = stack1;
                    }
                    List::Cons(_, Property::Seq(index), stack1) => {
                        let prop1 = _binary_seq(from_node, None);
                        props = props.insert(mem, &total, *index, prop1);
                        cur = stack1;
                    }
                }
            }
            props
        }
        fn _update<'b, 'a: 'b>(
            mem: &'b Bump,
            node: u64,
            props: &'a Graph<'a>,
            scope: &'b List<'b, Property<u64>>,
            stack: &'b List<'b, Property<u64>>,
        ) -> (&'b List<'b, Property<u64>>, &'b Graph<'b>) {
            // Walk scope and stack in lockstep: matching grp/seq scopes are kept
            // (collected as the common prefix); the first divergence closes the
            // remaining scope and opens the remaining stack.
            let mut matched: Vec<Property<u64>> = Vec::new();
            let mut sc = scope;
            let mut st = stack;
            let (rest_stack, props_out): (&'b List<'b, Property<u64>>, &'b Graph<'b>) = loop {
                match (sc, st) {
                    (_, List::Nil) => break (_list::nil(mem), _close(mem, node, props, sc)),
                    (List::Nil, _) => break (st, _open(mem, node, props, st)),
                    (
                        List::Cons(_, Property::Grp(left), scope1),
                        List::Cons(_, Property::Grp(right), stack1),
                    ) => {
                        if left > right {
                            unreachable!("Invariant")
                        }
                        if left == right {
                            matched.push(_unary_grp(*left));
                            sc = scope1;
                            st = stack1;
                        } else {
                            let props1 = _close(mem, node, props, sc);
                            break (st, _open(mem, node, props1, st));
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
                            matched.push(_unary_seq(*left));
                            sc = scope1;
                            st = stack1;
                        } else {
                            let props1 = _close(mem, node, props, sc);
                            break (st, _open(mem, node, props1, st));
                        }
                    }
                    _ => {
                        let props1 = _close(mem, node, props, sc);
                        break (st, _open(mem, node, props1, st));
                    }
                }
            };
            // Prepend the matched prefix (outermost first) onto the rest.
            let mut result = rest_stack;
            for prop in matched.iter().rev() {
                result = _list::cons(mem, *prop, result);
            }
            (result, props_out)
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
            // Materialize each closed grp/seq property as a graph edge.
            let mut cur = props;
            loop {
                match cur {
                    List::Nil => break,
                    List::Cons(_, Property::Grp((from_index, Some(to_index))), props1) => {
                        if from_index != to_index {
                            let from_node = nodes.get_unsafe(*from_index);
                            let to_node = nodes.get_unsafe(*to_index);
                            let curr = make_edge(mem, _unit_grp(), from_node, to_node);
                            _push_ins(curr, to_node);
                            _push_outs(curr, from_node);
                        }
                        cur = props1;
                    }
                    List::Cons(_, Property::Seq((from_index, Some(to_index))), props1) => {
                        if from_index != to_index {
                            let from_node = nodes.get_unsafe(*from_index);
                            let to_node = nodes.get_unsafe(*to_index);
                            let curr = make_edge(mem, _unit_seq(), from_node, to_node);
                            _push_ins(curr, to_node);
                            _push_outs(curr, from_node);
                        }
                        cur = props1;
                    }
                    _ => unreachable!("Invariant"),
                }
            }
        }
        fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b GraphDoc<'b> {
            // Walk the linear FixedDoc spine, graphifying each line's object.
            type Line<'b> = (&'b List<'b, &'b GraphNode<'b>>, &'b List<'b, bool>);
            let mut breaks: Vec<Line<'b>> = Vec::new();
            let mut cur = doc;
            loop {
                match cur {
                    FixedDoc::Eod => break,
                    FixedDoc::Break(obj, doc1) => {
                        let (nodes2, pads1, props1) = _visit_obj(mem, obj);
                        // Map::values yields the properties in key order; the
                        // reference passes it straight to _transpose.
                        let props2 = props1.values(mem);
                        _transpose(mem, nodes2, props2);
                        breaks.push((nodes2, pads1));
                        cur = doc1;
                    }
                }
            }
            let mut gdoc: &'b GraphDoc<'b> = _eod(mem);
            for &(nodes2, pads1) in breaks.iter().rev() {
                gdoc = _break(mem, nodes2, pads1, gdoc);
            }
            gdoc
        }
        #[allow(clippy::type_complexity)]
        fn _visit_obj<'b, 'a: 'b>(
            mem: &'b Bump,
            obj: &'a FixedObj<'a>,
        ) -> (
            &'b List<'b, &'b GraphNode<'b>>,
            &'b List<'b, bool>,
            &'b Graph<'b>,
        ) {
            // Walk the object's item chain, assigning indices and threading the
            // scope stack and the open-scope property map.
            let mut nodes_vec: Vec<&'b GraphNode<'b>> = Vec::new();
            let mut pads_vec: Vec<bool> = Vec::new();
            let mut index: u64 = 0;
            let mut scope: &'b List<'b, Property<u64>> = _list::nil(mem);
            let mut props: &'b Graph<'b> = _map::empty(mem);
            let mut cur = obj;
            let final_props: &'b Graph<'b> = loop {
                match cur {
                    FixedObj::Next(item, comp, obj1) => {
                        let term1 = match item {
                            FixedItem::Term(term) => _visit_term(mem, term),
                            FixedItem::Fix(fix) => {
                                let (fix1, scope1, props1) =
                                    _visit_fix(mem, fix, index, scope, props);
                                scope = scope1;
                                props = props1;
                                _fix(mem, fix1)
                            }
                        };
                        nodes_vec.push(make_node(mem, index, term1));
                        let (stack, pad) = _lift_stack(mem, comp);
                        pads_vec.push(pad);
                        let (scope2, props2) = _update(mem, index, props, scope, stack);
                        scope = scope2;
                        props = props2;
                        index += 1;
                        cur = obj1;
                    }
                    FixedObj::Last(item) => {
                        let term1 = match item {
                            FixedItem::Term(term) => _visit_term(mem, term),
                            FixedItem::Fix(fix) => {
                                let (fix1, scope1, props1) =
                                    _visit_fix(mem, fix, index, scope, props);
                                scope = scope1;
                                props = props1;
                                _fix(mem, fix1)
                            }
                        };
                        nodes_vec.push(make_node(mem, index, term1));
                        break _close(mem, index, props, scope);
                    }
                }
            };
            (
                _list_of(mem, &nodes_vec),
                _list_of(mem, &pads_vec),
                final_props,
            )
        }
        fn _visit_term<'b, 'a: 'b>(mem: &'b Bump, term: &'a FixedTerm<'a>) -> &'b GraphTerm<'b> {
            enum Wrap {
                Nest,
                Pack(u64),
            }
            let mut wraps: Vec<Wrap> = Vec::new();
            let mut cur = term;
            let mut val: &'b GraphTerm<'b> = loop {
                match cur {
                    FixedTerm::Null => break _null(mem),
                    FixedTerm::Text(data) => break _text(mem, data),
                    FixedTerm::Nest(term1) => {
                        wraps.push(Wrap::Nest);
                        cur = term1;
                    }
                    FixedTerm::Pack(index, term1) => {
                        wraps.push(Wrap::Pack(*index));
                        cur = term1;
                    }
                }
            };
            while let Some(wrap) = wraps.pop() {
                val = match wrap {
                    Wrap::Nest => _nest(mem, val),
                    Wrap::Pack(index) => _pack(mem, index, val),
                };
            }
            val
        }
        fn _visit_fix<'b, 'a: 'b>(
            mem: &'b Bump,
            fix: &'a FixedFix<'a>,
            index: u64,
            scope: &'b List<'b, Property<u64>>,
            props: &'b Graph<'b>,
        ) -> (&'b GraphFix<'b>, &'b List<'b, Property<u64>>, &'b Graph<'b>) {
            // Walk the fix chain forward, threading scope/props; rebuild the
            // GraphFix bottom-up from the recorded (term, pad) pairs.
            let mut recorded: Vec<(&'b GraphTerm<'b>, bool)> = Vec::new();
            let mut cur = fix;
            let mut scope: &'b List<'b, Property<u64>> = scope;
            let mut props: &'b Graph<'b> = props;
            let last_term: &'b GraphTerm<'b> = loop {
                match cur {
                    FixedFix::Next(term, comp, fix1) => {
                        let term1 = _visit_term(mem, term);
                        let (stack, pad) = _lift_stack(mem, comp);
                        let (scope1, props1) = _update(mem, index, props, scope, stack);
                        recorded.push((term1, pad));
                        scope = scope1;
                        props = props1;
                        cur = fix1;
                    }
                    FixedFix::Last(term) => break _visit_term(mem, term),
                }
            };
            let mut gfix: &'b GraphFix<'b> = _fix_last(mem, last_term);
            for &(term1, pad) in recorded.iter().rev() {
                gfix = _fix_next(mem, term1, gfix, pad);
            }
            (gfix, scope, props)
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
                    let mut cur = ins;
                    while let Some(edge) = cur {
                        edge.target.set(node);
                        cur = edge.ins_next.get();
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
        // Walks the outs edges from `outs`, moving each Seq edge out of the way,
        // and returns the first Grp edge (or None if the edges are exhausted).
        fn _resolve<'a>(
            edge: &'a GraphEdge<'a>,
            outs: &'a GraphEdge<'a>,
        ) -> Option<&'a GraphEdge<'a>> {
            let mut maybe_curr = Some(outs);
            let mut edge = edge;
            loop {
                match maybe_curr {
                    None => break None,
                    Some(curr) => match curr.prop {
                        Property::Grp(()) => break Some(curr),
                        Property::Seq(()) => {
                            let curr1 = curr.outs_next.get();
                            _move_out(curr, edge);
                            edge = curr;
                            maybe_curr = curr1;
                        }
                    },
                }
            }
        }
        fn _leftmost<'a>(head: &'a GraphEdge<'a>) -> &'a GraphEdge<'a> {
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
        fn _visit_doc<'a>(mem: &'a Bump, doc: &'a GraphDoc<'a>) -> &'a GraphDoc<'a> {
            // Walk the linear spine, solving each line's graph in place.
            type Line<'a> = (&'a List<'a, &'a GraphNode<'a>>, &'a List<'a, bool>);
            let mut breaks: Vec<Line<'a>> = Vec::new();
            let mut cur = doc;
            loop {
                match cur {
                    GraphDoc::Eod => break,
                    GraphDoc::Break(nodes, pads, doc1) => {
                        _visit_node(nodes.length(), nodes);
                        breaks.push((nodes, pads));
                        cur = doc1;
                    }
                }
            }
            let mut gdoc: &'a GraphDoc<'a> = _eod(mem);
            for &(nodes, pads) in breaks.iter().rev() {
                gdoc = _break(mem, nodes, pads, gdoc);
            }
            gdoc
        }
        fn _visit_node<'a>(count: u64, nodes: &'a List<'a, &'a GraphNode<'a>>) {
            let mut index = 0u64;
            while index != count {
                let node = nodes.get_unsafe(index);
                match (
                    (node.ins_head.get(), node.ins_tail.get()),
                    (node.outs_head.get(), node.outs_tail.get()),
                ) {
                    ((Some(ins_head), Some(ins_tail)), (Some(outs_head), Some(_outs_tail))) => {
                        let ins_first = _leftmost(ins_head);
                        if let Some(outs_head1) = _resolve(ins_first, outs_head) {
                            _move_ins(ins_head, ins_tail, outs_head1);
                        }
                    }
                    ((Some(_), None), _)
                    | ((None, Some(_)), _)
                    | (_, (Some(_), None))
                    | (_, (None, Some(_))) => unreachable!("Invariant"),
                    (_, _) => {}
                }
                index += 1;
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
        fn _topology<'b, 'a: 'b>(
            mem: &'b Bump,
            nodes: &'a List<'a, &'a GraphNode<'a>>,
        ) -> TopologyResult<'b> {
            fn _num_ins(node: &GraphNode) -> u64 {
                let mut num = 0u64;
                let mut cur = node.ins_head.get();
                while let Some(edge) = cur {
                    num += 1;
                    cur = edge.ins_next.get();
                }
                num
            }
            fn _prop_outs<'b, 'a: 'b>(
                mem: &'b Bump,
                node: &'a GraphNode<'a>,
            ) -> &'b List<'b, Property<()>> {
                let mut props: Vec<Property<()>> = Vec::new();
                let mut cur = node.outs_head.get();
                while let Some(edge) = cur {
                    props.push(edge.prop);
                    cur = edge.outs_next.get();
                }
                _list_of(mem, &props)
            }
            let mut terms: Vec<&'b GraphTerm<'b>> = Vec::new();
            let mut ins: Vec<u64> = Vec::new();
            let mut outs: Vec<&'b List<'b, Property<()>>> = Vec::new();
            let len = nodes.length();
            let mut index = 0u64;
            while index < len {
                let node = nodes.get_unsafe(index);
                terms.push(copy_graph_term(mem, node.term));
                ins.push(_num_ins(node));
                outs.push(_prop_outs(mem, node));
                index += 1;
            }
            (
                _list_of(mem, &terms),
                _list_of(mem, &ins),
                _list_of(mem, &outs),
            )
        }
        // Composes `partial` into the top continuation, then pushes a grp/seq
        // continuation for each property.
        fn _open<'b>(
            mem: &'b Bump,
            props: &'b List<'b, Property<()>>,
            stack: RStack<'b>,
            partial: RPartial<'b>,
        ) -> RStack<'b> {
            match stack {
                List::Cons(_, top, stack1) => {
                    let top1: RCont<'b> = _list::cons(mem, RStep::Partial(partial), top);
                    let mut result: RStack<'b> = _list::cons(mem, top1, stack1);
                    let mut cur = props;
                    while let List::Cons(_, prop, props1) = cur {
                        let cont: RCont<'b> = match prop {
                            Property::Grp(()) => _list::cons(mem, RStep::Grp, _list::nil(mem)),
                            Property::Seq(()) => _list::cons(mem, RStep::Seq, _list::nil(mem)),
                        };
                        result = _list::cons(mem, cont, result);
                        cur = props1;
                    }
                    result
                }
                _ => unreachable!("Invariant"),
            }
        }
        // Pops `count` continuations, applying each to the accumulating object.
        fn _close<'b>(
            mem: &'b Bump,
            count: u64,
            stack: RStack<'b>,
            term: &'b RebuildObj<'b>,
        ) -> (RStack<'b>, &'b RebuildObj<'b>) {
            let mut count = count;
            let mut stack = stack;
            let mut result = term;
            while count > 0 {
                match stack {
                    List::Cons(_, top, stack1) => {
                        result = _apply_rcont(mem, top, result);
                        stack = stack1;
                        count -= 1;
                    }
                    _ => unreachable!("Invariant"),
                }
            }
            (stack, result)
        }
        fn _final<'b>(
            mem: &'b Bump,
            stack: RStack<'b>,
            term: &'b RebuildObj<'b>,
        ) -> &'b RebuildObj<'b> {
            match stack {
                List::Cons(_, last, List::Nil) => _apply_rcont(mem, last, term),
                _ => unreachable!("Invariant"),
            }
        }
        fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a GraphDoc<'a>) -> &'b RebuildDoc<'b> {
            // Walk the linear spine, rebuilding each line's object.
            let mut objs: Vec<&'b RebuildObj<'b>> = Vec::new();
            let mut cur = doc;
            loop {
                match cur {
                    GraphDoc::Eod => break,
                    GraphDoc::Break(nodes, pads, doc1) => {
                        let (terms, ins, outs) = _topology(mem, nodes);
                        // The initial continuation is the identity (an empty
                        // step list); the initial partial is the identity too.
                        let id_cont: RCont<'b> = _list::nil(mem);
                        let stack: RStack<'b> = _list::cons(mem, id_cont, _list::nil(mem));
                        let partial: RPartial<'b> = _list::nil(mem);
                        objs.push(_visit_line(mem, terms, pads, ins, outs, stack, partial));
                        cur = doc1;
                    }
                }
            }
            let mut rdoc: &'b RebuildDoc<'b> = _eod(mem);
            for &obj in objs.iter().rev() {
                rdoc = _break(mem, obj, rdoc);
            }
            rdoc
        }
        #[allow(clippy::too_many_arguments)]
        fn _visit_line<'b, 'a: 'b>(
            mem: &'b Bump,
            terms: &'a List<'a, &'a GraphTerm<'a>>,
            pads: &'a List<'a, bool>,
            ins: &'a List<'a, u64>,
            outs: &'a List<'a, &'a List<'a, Property<()>>>,
            stack: RStack<'b>,
            partial: RPartial<'b>,
        ) -> &'b RebuildObj<'b> {
            // Walk the aligned (term, pad, in-degree, out-props) lists, threading
            // the continuation stack and the left composition spine (partial).
            let mut terms = terms;
            let mut pads = pads;
            let mut ins = ins;
            let mut outs = outs;
            let mut stack = stack;
            let mut partial = partial;
            loop {
                match (terms, pads) {
                    // Final term of the line (fixed or plain).
                    (List::Cons(_, GraphTerm::Fix(fix), List::Nil), List::Nil) => {
                        let fobj = _fix(mem, _visit_fix(mem, fix));
                        return match (ins, outs) {
                            (List::Cons(_, 0, List::Nil), List::Cons(_, List::Nil, List::Nil)) => {
                                _final(mem, stack, _apply_rpartial(mem, partial, fobj))
                            }
                            (
                                List::Cons(_, in_props, List::Nil),
                                List::Cons(_, List::Nil, List::Nil),
                            ) => {
                                let (stack1, fix2) = _close(
                                    mem,
                                    *in_props,
                                    stack,
                                    _apply_rpartial(mem, partial, fobj),
                                );
                                _final(mem, stack1, fix2)
                            }
                            (_, _) => unreachable!("Invariant"),
                        };
                    }
                    (List::Cons(_, term, List::Nil), List::Nil) => {
                        let tobj = _term(mem, _visit_term(mem, term));
                        return match (ins, outs) {
                            (List::Cons(_, 0, List::Nil), List::Cons(_, List::Nil, List::Nil)) => {
                                _final(mem, stack, _apply_rpartial(mem, partial, tobj))
                            }
                            (
                                List::Cons(_, in_props, List::Nil),
                                List::Cons(_, List::Nil, List::Nil),
                            ) => {
                                let (stack1, term2) = _close(
                                    mem,
                                    *in_props,
                                    stack,
                                    _apply_rpartial(mem, partial, tobj),
                                );
                                _final(mem, stack1, term2)
                            }
                            (_, _) => unreachable!("Invariant"),
                        };
                    }
                    // A term with a following composition (fixed or plain).
                    (List::Cons(_, GraphTerm::Fix(fix), terms1), List::Cons(_, pad, pads1)) => {
                        let fobj = _fix(mem, _visit_fix(mem, fix));
                        match (ins, outs) {
                            (List::Cons(_, 0, ins1), List::Cons(_, List::Nil, outs1)) => {
                                partial = _list::cons(mem, (fobj, *pad), partial);
                                terms = terms1;
                                pads = pads1;
                                ins = ins1;
                                outs = outs1;
                            }
                            (List::Cons(_, in_props, ins1), List::Cons(_, List::Nil, outs1)) => {
                                let (stack1, fix2) = _close(
                                    mem,
                                    *in_props,
                                    stack,
                                    _apply_rpartial(mem, partial, fobj),
                                );
                                partial = _list::cons(mem, (fix2, *pad), _list::nil(mem));
                                stack = stack1;
                                terms = terms1;
                                pads = pads1;
                                ins = ins1;
                                outs = outs1;
                            }
                            (List::Cons(_, 0, ins1), List::Cons(_, out_props, outs1)) => {
                                stack = _open(mem, out_props, stack, partial);
                                partial = _list::cons(mem, (fobj, *pad), _list::nil(mem));
                                terms = terms1;
                                pads = pads1;
                                ins = ins1;
                                outs = outs1;
                            }
                            (_, _) => unreachable!("Invariant"),
                        }
                    }
                    (List::Cons(_, term, terms1), List::Cons(_, pad, pads1)) => {
                        let tobj = _term(mem, _visit_term(mem, term));
                        match (ins, outs) {
                            (List::Cons(_, 0, ins1), List::Cons(_, List::Nil, outs1)) => {
                                partial = _list::cons(mem, (tobj, *pad), partial);
                                terms = terms1;
                                pads = pads1;
                                ins = ins1;
                                outs = outs1;
                            }
                            (List::Cons(_, in_props, ins1), List::Cons(_, List::Nil, outs1)) => {
                                let (stack1, term2) = _close(
                                    mem,
                                    *in_props,
                                    stack,
                                    _apply_rpartial(mem, partial, tobj),
                                );
                                partial = _list::cons(mem, (term2, *pad), _list::nil(mem));
                                stack = stack1;
                                terms = terms1;
                                pads = pads1;
                                ins = ins1;
                                outs = outs1;
                            }
                            (List::Cons(_, 0, ins1), List::Cons(_, out_props, outs1)) => {
                                stack = _open(mem, out_props, stack, partial);
                                partial = _list::cons(mem, (tobj, *pad), _list::nil(mem));
                                terms = terms1;
                                pads = pads1;
                                ins = ins1;
                                outs = outs1;
                            }
                            (_, _) => unreachable!("Invariant"),
                        }
                    }
                    (_, _) => unreachable!("Invariant"),
                }
            }
        }
        fn _visit_term<'b, 'a: 'b>(mem: &'b Bump, term: &'a GraphTerm<'a>) -> &'b RebuildTerm<'b> {
            enum Wrap {
                Nest,
                Pack(u64),
            }
            let mut wraps: Vec<Wrap> = Vec::new();
            let mut cur = term;
            let mut val: &'b RebuildTerm<'b> = loop {
                match cur {
                    GraphTerm::Null => break _null(mem),
                    GraphTerm::Text(data) => break _text(mem, data),
                    GraphTerm::Nest(term1) => {
                        wraps.push(Wrap::Nest);
                        cur = term1;
                    }
                    GraphTerm::Pack(index, term1) => {
                        wraps.push(Wrap::Pack(*index));
                        cur = term1;
                    }
                    GraphTerm::Fix(_fix) => unreachable!("Invariant"),
                }
            };
            while let Some(wrap) = wraps.pop() {
                val = match wrap {
                    Wrap::Nest => _nest(mem, val),
                    Wrap::Pack(index) => _pack(mem, index, val),
                };
            }
            val
        }
        fn _visit_fix<'b, 'a: 'b>(mem: &'b Bump, fix: &'a GraphFix<'a>) -> &'b RebuildFix<'b> {
            // Walk the fix chain, then rebuild the RebuildFix bottom-up.
            let mut recorded: Vec<(&'b RebuildTerm<'b>, bool)> = Vec::new();
            let mut cur = fix;
            let last: &'b RebuildTerm<'b> = loop {
                match cur {
                    GraphFix::Last(term) => break _visit_term(mem, term),
                    GraphFix::Next(term, fix1, pad) => {
                        recorded.push((_visit_term(mem, term), *pad));
                        cur = fix1;
                    }
                }
            };
            let mut rfix: &'b RebuildFix<'b> = _fix_term(mem, last);
            for &(term1, pad) in recorded.iter().rev() {
                rfix = _fix_comp(mem, _fix_term(mem, term1), rfix, pad);
            }
            rfix
        }
        _visit_doc(mem, doc)
    }
    let doc1 = _graphify(mem, doc);
    let doc2 = _solve(mem, doc1);
    _rebuild(mem, doc2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{FixedItem, FixedTerm};

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration across
    /// all three phases (graphify, solve, rebuild).
    const DEEP: usize = 50_000;

    #[test]
    fn structurize_handles_deep_comp_line() {
        let mem = Bump::new();
        // A single line of many plain compositions (no grp/seq scopes). Indexed
        // node access in solve/rebuild makes this path O(n^2), so use a depth
        // that is well past the ~400-level overflow threshold but still quick.
        let depth = 20_000usize;
        let mut obj: &FixedObj = mem.alloc(FixedObj::Last(
            mem.alloc(FixedItem::Term(mem.alloc(FixedTerm::Text("z")))),
        ));
        for _ in 0..depth {
            obj = mem.alloc(FixedObj::Next(
                mem.alloc(FixedItem::Term(mem.alloc(FixedTerm::Text("y")))),
                mem.alloc(FixedComp::Comp(false)),
                obj,
            ));
        }
        let doc: &FixedDoc = mem.alloc(FixedDoc::Break(obj, mem.alloc(FixedDoc::Eod)));
        let out = structurize(&mem, doc);
        // One line, rebuilt as a right-nested composition spine.
        let RebuildDoc::Break(robj, _) = out else {
            panic!("expected a break")
        };
        let mut count = 0usize;
        let mut cur: &RebuildObj = robj;
        while let RebuildObj::Comp(_left, right, _pad) = cur {
            count += 1;
            cur = right;
        }
        assert_eq!(count, depth);
    }

    #[test]
    fn structurize_handles_deep_nest_term() {
        let mem = Bump::new();
        // A deep Nest term exercises _visit_term and copy_graph_term at depth.
        let mut term: &FixedTerm = mem.alloc(FixedTerm::Text("x"));
        for _ in 0..DEEP {
            term = mem.alloc(FixedTerm::Nest(term));
        }
        let obj: &FixedObj = mem.alloc(FixedObj::Last(mem.alloc(FixedItem::Term(term))));
        let doc: &FixedDoc = mem.alloc(FixedDoc::Break(obj, mem.alloc(FixedDoc::Eod)));
        let out = structurize(&mem, doc);
        let RebuildDoc::Break(RebuildObj::Term(t), _) = out else {
            panic!("expected a single term")
        };
        let mut count = 0usize;
        let mut cur: &RebuildTerm = t;
        while let RebuildTerm::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn structurize_handles_deep_fix_group() {
        let mem = Bump::new();
        // A deep fixed group exercises the fix trampolines and copy_graph_fix.
        let mut fix: &FixedFix = mem.alloc(FixedFix::Last(mem.alloc(FixedTerm::Text("z"))));
        for _ in 0..DEEP {
            fix = mem.alloc(FixedFix::Next(
                mem.alloc(FixedTerm::Text("y")),
                mem.alloc(FixedComp::Comp(false)),
                fix,
            ));
        }
        let obj: &FixedObj = mem.alloc(FixedObj::Last(mem.alloc(FixedItem::Fix(fix))));
        let doc: &FixedDoc = mem.alloc(FixedDoc::Break(obj, mem.alloc(FixedDoc::Eod)));
        let out = structurize(&mem, doc);
        let RebuildDoc::Break(RebuildObj::Fix(rfix), _) = out else {
            panic!("expected a fix object")
        };
        let mut count = 0usize;
        let mut cur: &RebuildFix = rfix;
        while let RebuildFix::Comp(_left, right, _pad) = cur {
            count += 1;
            cur = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn structurize_handles_long_doc_spine() {
        let mem = Bump::new();
        // Many document rows exercise the doc-spine walks in all three phases.
        let mut doc: &FixedDoc = mem.alloc(FixedDoc::Eod);
        for _ in 0..DEEP {
            let obj: &FixedObj = mem.alloc(FixedObj::Last(
                mem.alloc(FixedItem::Term(mem.alloc(FixedTerm::Text("x")))),
            ));
            doc = mem.alloc(FixedDoc::Break(obj, doc));
        }
        let out = structurize(&mem, doc);
        let mut count = 0usize;
        let mut cur = out;
        while let RebuildDoc::Break(_, rest) = cur {
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, RebuildDoc::Eod));
        assert_eq!(count, DEEP);
    }
}
