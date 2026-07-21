//! Phase 3 of structurize: GraphDoc → RebuildDoc.
//!
//! Reads the solved scope graph per line and rebuilds an explicit composition
//! spine (grp/seq wrappers and left compositions) as a RebuildDoc.

use crate::compiler::types::{
    GraphDoc, GraphFix, GraphNode, GraphTerm, Property, RebuildDoc, RebuildFix, RebuildObj,
    RebuildTerm, TopologyResult,
};
use bumpalo::Bump;

// Defunctionalized rebuild continuations (replacing the `partial` closure and
// the RebuildCont closure stack used by `_visit_line`).
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

pub(super) fn rebuild<'b, 'a: 'b>(mem: &'b Bump, doc: &'a GraphDoc<'a>) -> &'b RebuildDoc<'b> {
    fn _topology<'b, 'a: 'b>(mem: &'b Bump, nodes: &'a [&'a GraphNode<'a>]) -> TopologyResult<'b> {
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
        let mut rdoc: &'b RebuildDoc<'b> = mem.alloc(RebuildDoc::Eod);
        for &obj in objs.iter().rev() {
            rdoc = mem.alloc(RebuildDoc::Break(obj, rdoc));
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
                GraphTerm::Fix(fix) => mem.alloc(RebuildObj::Fix(_visit_fix(mem, fix))),
                _ => mem.alloc(RebuildObj::Term(_visit_term(mem, term))),
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
                GraphTerm::Null => break mem.alloc(RebuildTerm::Null),
                GraphTerm::Text(data) => break mem.alloc(RebuildTerm::Text(data)),
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
                Wrap::Nest => mem.alloc(RebuildTerm::Nest(val)),
                Wrap::Pack(index) => mem.alloc(RebuildTerm::Pack(index, val)),
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
        let mut rfix: &'b RebuildFix<'b> = mem.alloc(RebuildFix::Term(last));
        for &(term1, pad) in recorded.iter().rev() {
            rfix = mem.alloc(RebuildFix::Comp(
                mem.alloc(RebuildFix::Term(term1)),
                rfix,
                pad,
            ));
        }
        rfix
    }
    _visit_doc(mem, doc)
}
