//! Pass 5: FixedDoc → RebuildDoc (rebuild with graph structure)

use crate::compiler::types::{
    FixedComp, FixedDoc, FixedFix, FixedItem, FixedObj, FixedTerm, GraphDoc, GraphEdge, GraphFix,
    GraphNode, GraphTerm, Property, RebuildDoc, RebuildFix, RebuildObj, RebuildTerm,
    TopologyResult,
};
use bumpalo::Bump;
use std::cell::Cell;
use std::collections::BTreeMap;

// Defunctionalized rebuild continuations (replacing the `partial` closure and
// the RebuildCont closure stack used by `_rebuild::_visit_line`).
//
// A partial is a left composition spine. It is stored so `push` appends the
// innermost (most recently added) element, i.e. the tail is `[.., (xk,pk)]` with
// `(xk,pk)` innermost; `_apply_rpartial` folds from the end, yielding
// `Comp(x0, Comp(x1, .. Comp(xk, obj, pk) .., p1), p0)`.
type RPartial<'b> = Vec<(&'b RebuildObj<'b>, bool)>;

// A continuation step. A continuation is a stack of steps stored so `push`
// appends the head (the step applied first / innermost); `_apply_rcont` folds
// from the end. RStack is the stack of continuations, top at the end.
#[derive(Debug)]
enum RStep<'b> {
    Grp,
    Seq,
    Partial(RPartial<'b>),
}
type RCont<'b> = Vec<RStep<'b>>;
type RStack<'b> = Vec<RCont<'b>>;

// Applies a partial spine to an object (innermost element first).
fn _apply_rpartial<'b>(
    mem: &'b Bump,
    partial: &[(&'b RebuildObj<'b>, bool)],
    obj: &'b RebuildObj<'b>,
) -> &'b RebuildObj<'b> {
    let mut result = obj;
    for &(left, pad) in partial.iter().rev() {
        result = mem.alloc(RebuildObj::Comp(left, result, pad));
    }
    result
}

// Applies a continuation (stack of steps) to an object (head step first).
fn _apply_rcont<'b>(
    mem: &'b Bump,
    cont: &[RStep<'b>],
    obj: &'b RebuildObj<'b>,
) -> &'b RebuildObj<'b> {
    let mut result = obj;
    for step in cont.iter().rev() {
        result = match step {
            RStep::Grp => mem.alloc(RebuildObj::Grp(result)),
            RStep::Seq => mem.alloc(RebuildObj::Seq(result)),
            RStep::Partial(partial) => _apply_rpartial(mem, partial, result),
        };
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
        nodes: &'a [&'a GraphNode<'a>],
        pads: &'a [bool],
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
        fn _lift_stack(comp: &FixedComp) -> (Vec<Property<u64>>, bool) {
            // Linear chain of grp/seq wrappers around a leaf Comp(pad). Index 0
            // is the outermost wrapper (what used to be the list head).
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
            (props, pad)
        }
        // The open-scope property map is keyed by scope index. It is threaded
        // linearly (each update replaces the binding; no earlier version is ever
        // retained), so a plain owned `BTreeMap` mutated in place is a faithful
        // replacement for the former persistent map. `BTreeMap` also gives the
        // key-ordered `values()` iteration `_transpose` depends on.
        type Graph = BTreeMap<u64, Property<(u64, Option<u64>)>>;
        fn _close(to_node: u64, mut props: Graph, stack: &[Property<u64>]) -> Graph {
            // Close each open grp/seq scope on the stack by recording to_node.
            for prop in stack {
                match prop {
                    Property::Grp(index) => match props[index] {
                        Property::Seq(_) => unreachable!("Invariant"),
                        Property::Grp((from_node, _to_node)) => {
                            props.insert(*index, _binary_grp(from_node, Some(to_node)));
                        }
                    },
                    Property::Seq(index) => match props[index] {
                        Property::Grp(_) => unreachable!("Invariant"),
                        Property::Seq((from_node, _to_node)) => {
                            props.insert(*index, _binary_seq(from_node, Some(to_node)));
                        }
                    },
                }
            }
            props
        }
        fn _open(from_node: u64, mut props: Graph, stack: &[Property<u64>]) -> Graph {
            // Open a fresh grp/seq scope on the stack, anchored at from_node.
            for prop in stack {
                match prop {
                    Property::Grp(index) => {
                        props.insert(*index, _binary_grp(from_node, None));
                    }
                    Property::Seq(index) => {
                        props.insert(*index, _binary_seq(from_node, None));
                    }
                }
            }
            props
        }
        fn _update(
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
                        props = _close(node, props, &scope[k..]);
                        break &[];
                    }
                    (None, _) => {
                        props = _open(node, props, &stack[k..]);
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
                            props = _close(node, props, &scope[k..]);
                            props = _open(node, props, &stack[k..]);
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
        fn _transpose<'a>(
            mem: &'a Bump,
            nodes: &'a [&'a GraphNode<'a>],
            props: &[Property<(u64, Option<u64>)>],
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
            for prop in props {
                match prop {
                    Property::Grp((from_index, Some(to_index))) => {
                        if from_index != to_index {
                            let from_node = nodes[*from_index as usize];
                            let to_node = nodes[*to_index as usize];
                            let curr = make_edge(mem, _unit_grp(), from_node, to_node);
                            _push_ins(curr, to_node);
                            _push_outs(curr, from_node);
                        }
                    }
                    Property::Seq((from_index, Some(to_index))) => {
                        if from_index != to_index {
                            let from_node = nodes[*from_index as usize];
                            let to_node = nodes[*to_index as usize];
                            let curr = make_edge(mem, _unit_seq(), from_node, to_node);
                            _push_ins(curr, to_node);
                            _push_outs(curr, from_node);
                        }
                    }
                    _ => unreachable!("Invariant"),
                }
            }
        }
        fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a FixedDoc<'a>) -> &'b GraphDoc<'b> {
            // Walk the linear FixedDoc spine, graphifying each line's object.
            type Line<'b> = (&'b [&'b GraphNode<'b>], &'b [bool]);
            let mut breaks: Vec<Line<'b>> = Vec::new();
            let mut cur = doc;
            loop {
                match cur {
                    FixedDoc::Eod => break,
                    FixedDoc::Break(obj, doc1) => {
                        let (nodes2, pads1, props1) = _visit_obj(mem, obj);
                        // BTreeMap::values yields the properties in key order,
                        // which is the order _transpose consumes them in.
                        let props2: Vec<Property<(u64, Option<u64>)>> =
                            props1.values().copied().collect();
                        _transpose(mem, nodes2, &props2);
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
                        let (stack, pad) = _lift_stack(comp);
                        pads_vec.push(pad);
                        let (scope2, props2) = _update(index, props, &scope, &stack);
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
                        break _close(index, props, &scope);
                    }
                }
            };
            (
                mem.alloc_slice_copy(&nodes_vec),
                mem.alloc_slice_copy(&pads_vec),
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
                        let term1 = _visit_term(mem, term);
                        let (stack, pad) = _lift_stack(comp);
                        let (scope1, props1) = _update(index, props, &scope, &stack);
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
            type Line<'a> = (&'a [&'a GraphNode<'a>], &'a [bool]);
            let mut breaks: Vec<Line<'a>> = Vec::new();
            let mut cur = doc;
            loop {
                match cur {
                    GraphDoc::Eod => break,
                    GraphDoc::Break(nodes, pads, doc1) => {
                        _visit_node(nodes);
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
        fn _visit_node<'a>(nodes: &'a [&'a GraphNode<'a>]) {
            for node in nodes {
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
            nodes: &'a [&'a GraphNode<'a>],
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
            fn _prop_outs(node: &GraphNode) -> Vec<Property<()>> {
                let mut props: Vec<Property<()>> = Vec::new();
                let mut cur = node.outs_head.get();
                while let Some(edge) = cur {
                    props.push(edge.prop);
                    cur = edge.outs_next.get();
                }
                props
            }
            let mut terms: Vec<&'b GraphTerm<'b>> = Vec::new();
            let mut ins: Vec<u64> = Vec::new();
            let mut outs: Vec<Vec<Property<()>>> = Vec::new();
            for &node in nodes {
                terms.push(copy_graph_term(mem, node.term));
                ins.push(_num_ins(node));
                outs.push(_prop_outs(node));
            }
            (terms, ins, outs)
        }
        // Composes `partial` into the top continuation, then pushes a grp/seq
        // continuation for each property.
        fn _open<'b>(
            props: &[Property<()>],
            mut stack: RStack<'b>,
            partial: RPartial<'b>,
        ) -> RStack<'b> {
            // Prepend a Partial step onto the current top continuation (a `push`
            // in the reversed-storage convention), then push a fresh single-step
            // continuation per property.
            let mut top = stack.pop().expect("Invariant");
            top.push(RStep::Partial(partial));
            stack.push(top);
            for prop in props {
                stack.push(match prop {
                    Property::Grp(()) => vec![RStep::Grp],
                    Property::Seq(()) => vec![RStep::Seq],
                });
            }
            stack
        }
        // Pops `count` continuations, applying each to the accumulating object.
        fn _close<'b>(
            mem: &'b Bump,
            count: u64,
            mut stack: RStack<'b>,
            term: &'b RebuildObj<'b>,
        ) -> (RStack<'b>, &'b RebuildObj<'b>) {
            let mut result = term;
            for _ in 0..count {
                let top = stack.pop().expect("Invariant");
                result = _apply_rcont(mem, &top, result);
            }
            (stack, result)
        }
        fn _final<'b>(
            mem: &'b Bump,
            stack: RStack<'b>,
            term: &'b RebuildObj<'b>,
        ) -> &'b RebuildObj<'b> {
            match stack.as_slice() {
                [last] => _apply_rcont(mem, last, term),
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
                        // The initial stack holds one identity continuation (an
                        // empty step list); the initial partial is empty too.
                        let stack: RStack<'b> = vec![Vec::new()];
                        let partial: RPartial<'b> = Vec::new();
                        objs.push(_visit_line(mem, &terms, pads, &ins, &outs, stack, partial));
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
        fn _visit_line<'b>(
            mem: &'b Bump,
            terms: &[&'b GraphTerm<'b>],
            pads: &[bool],
            ins: &[u64],
            outs: &[Vec<Property<()>>],
            mut stack: RStack<'b>,
            mut partial: RPartial<'b>,
        ) -> &'b RebuildObj<'b> {
            // Walk the aligned per-node (term, in-degree, out-props) slices,
            // threading the continuation stack and the left composition spine
            // (partial). `pads` has one fewer element: `pads[i]` is the pad
            // between `terms[i]` and `terms[i + 1]`.
            let n = terms.len();
            let mut i = 0;
            loop {
                let term = terms[i];
                let obj = match term {
                    GraphTerm::Fix(fix) => _fix(mem, _visit_fix(mem, fix)),
                    _ => _term(mem, _visit_term(mem, term)),
                };
                let in_deg = ins[i];
                let out_props = outs[i].as_slice();
                if i + 1 == n {
                    // Final term of the line: it never has out-properties.
                    if !out_props.is_empty() {
                        unreachable!("Invariant")
                    }
                    let applied = _apply_rpartial(mem, &partial, obj);
                    return if in_deg == 0 {
                        _final(mem, stack, applied)
                    } else {
                        let (stack1, obj2) = _close(mem, in_deg, stack, applied);
                        _final(mem, stack1, obj2)
                    };
                }
                let pad = pads[i];
                match (in_deg, out_props.is_empty()) {
                    // In-degree 0, no out-properties: extend the partial spine.
                    (0, true) => partial.push((obj, pad)),
                    // In-degree > 0, no out-properties: close the incoming scopes,
                    // then start a fresh partial from the closed object.
                    (_, true) => {
                        let applied = _apply_rpartial(mem, &partial, obj);
                        let (stack1, obj2) = _close(mem, in_deg, stack, applied);
                        stack = stack1;
                        partial = vec![(obj2, pad)];
                    }
                    // In-degree 0, has out-properties: open new scopes, then
                    // start a fresh partial from this object.
                    (0, false) => {
                        stack = _open(out_props, stack, partial);
                        partial = vec![(obj, pad)];
                    }
                    (_, false) => unreachable!("Invariant"),
                }
                i += 1;
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
