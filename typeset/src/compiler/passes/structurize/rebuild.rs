//! Phase 3 of structurize: GraphDoc → RebuildDoc.
//!
//! Reads the solved scope graph per line and rebuilds an explicit composition
//! spine (grp/seq wrappers and left compositions) as a RebuildDoc.

use super::graph::{GraphDoc, GraphFix, GraphNode, GraphTerm, NodeInfo, Property};
use crate::compiler::passes::term_chain::map_term_chain;
use crate::compiler::types::{RebuildDoc, RebuildFix, RebuildObj, Term};
use bumpalo::Bump;

// Defunctionalized rebuild continuations (replacing the `partial` closure and
// the RebuildCont closure stack used by `visit_line`).
//
// A partial is a left composition spine. It is stored so `push` appends the
// innermost (most recently added) element, i.e. the tail is `[.., (xk,pk)]` with
// `(xk,pk)` innermost; `apply_rpartial` folds from the end, yielding
// `Comp(x0, Comp(x1, .. Comp(xk, obj, pk) .., p1), p0)`.
type RPartial<'b> = Vec<(&'b RebuildObj<'b>, bool)>;

// A continuation step. A continuation is a stack of steps stored so `push`
// appends the head (the step applied first / innermost); `apply_rcont` folds
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
fn apply_rpartial<'b>(
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
fn apply_rcont<'b>(
    mem: &'b Bump,
    cont: &[RStep<'b>],
    obj: &'b RebuildObj<'b>,
) -> &'b RebuildObj<'b> {
    let mut result = obj;
    for step in cont.iter().rev() {
        result = match step {
            RStep::Grp => mem.alloc(RebuildObj::Grp(result)),
            RStep::Seq => mem.alloc(RebuildObj::Seq(result)),
            RStep::Partial(partial) => apply_rpartial(mem, partial, result),
        };
    }
    result
}

pub(super) fn rebuild<'b, 'a: 'b>(mem: &'b Bump, doc: &'a GraphDoc<'a>) -> &'b RebuildDoc<'b> {
    // Per-node info in node-index order. The node terms are read directly (not
    // copied): `GraphTerm`/`GraphFix` are covariant and `'a: 'b`, so a
    // `&'a GraphTerm<'a>` is already usable as `&'b GraphTerm<'b>`. `rebuild`
    // only reads them to emit fresh Term nodes, so no defensive copy is
    // needed.
    fn topology<'b, 'a: 'b>(nodes: &'a [&'a GraphNode<'a>]) -> Vec<NodeInfo<'b>> {
        fn num_ins(node: &GraphNode) -> u64 {
            let mut num = 0u64;
            let mut cur = node.ins_head.get();
            while let Some(edge) = cur {
                num += 1;
                cur = edge.ins_next.get();
            }
            num
        }
        fn prop_outs(node: &GraphNode) -> Vec<Property<()>> {
            let mut props: Vec<Property<()>> = Vec::new();
            let mut cur = node.outs_head.get();
            while let Some(edge) = cur {
                props.push(edge.prop);
                cur = edge.outs_next.get();
            }
            props
        }
        nodes
            .iter()
            .map(|&node| NodeInfo {
                term: node.term,
                in_degree: num_ins(node),
                outs: prop_outs(node),
            })
            .collect()
    }
    // Composes `partial` into the top continuation, then pushes a grp/seq
    // continuation for each property.
    fn open<'b>(
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
    fn close<'b>(
        mem: &'b Bump,
        count: u64,
        mut stack: RStack<'b>,
        term: &'b RebuildObj<'b>,
    ) -> (RStack<'b>, &'b RebuildObj<'b>) {
        let mut result = term;
        for _ in 0..count {
            let top = stack.pop().expect("Invariant");
            result = apply_rcont(mem, &top, result);
        }
        (stack, result)
    }
    fn finalize<'b>(
        mem: &'b Bump,
        stack: RStack<'b>,
        term: &'b RebuildObj<'b>,
    ) -> &'b RebuildObj<'b> {
        match stack.as_slice() {
            [last] => apply_rcont(mem, last, term),
            _ => unreachable!("Invariant"),
        }
    }
    fn visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a GraphDoc<'a>) -> &'b RebuildDoc<'b> {
        // Walk the linear spine, rebuilding each line's object.
        let mut objs: Vec<&'b RebuildObj<'b>> = Vec::new();
        let mut cur = doc;
        loop {
            match cur {
                GraphDoc::Eod => break,
                GraphDoc::Break(nodes, pads, doc1) => {
                    let info = topology(nodes);
                    // The initial stack holds one identity continuation (an
                    // empty step list); the initial partial is empty too.
                    let stack: RStack<'b> = vec![Vec::new()];
                    let partial: RPartial<'b> = Vec::new();
                    objs.push(visit_line(mem, &info, pads, stack, partial));
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
    fn visit_line<'b>(
        mem: &'b Bump,
        info: &[NodeInfo<'b>],
        pads: &[bool],
        mut stack: RStack<'b>,
        mut partial: RPartial<'b>,
    ) -> &'b RebuildObj<'b> {
        // Walk the per-node info, threading the continuation stack and the left
        // composition spine (partial). `pads` has one fewer element: `pads[i]`
        // is the pad between `info[i]` and `info[i + 1]`.
        let n = info.len();
        let mut i = 0;
        loop {
            let term = info[i].term;
            let obj = match term {
                GraphTerm::Fix(fix) => mem.alloc(RebuildObj::Fix(visit_fix(mem, fix))),
                _ => mem.alloc(RebuildObj::Term(map_term_chain(mem, term))),
            };
            let in_deg = info[i].in_degree;
            let out_props = info[i].outs.as_slice();
            if i + 1 == n {
                // Final term of the line: it never has out-properties.
                if !out_props.is_empty() {
                    unreachable!("Invariant")
                }
                let applied = apply_rpartial(mem, &partial, obj);
                return if in_deg == 0 {
                    finalize(mem, stack, applied)
                } else {
                    let (stack1, obj2) = close(mem, in_deg, stack, applied);
                    finalize(mem, stack1, obj2)
                };
            }
            let pad = pads[i];
            match (in_deg, out_props.is_empty()) {
                // In-degree 0, no out-properties: extend the partial spine.
                (0, true) => partial.push((obj, pad)),
                // In-degree > 0, no out-properties: close the incoming scopes,
                // then start a fresh partial from the closed object.
                (_, true) => {
                    let applied = apply_rpartial(mem, &partial, obj);
                    let (stack1, obj2) = close(mem, in_deg, stack, applied);
                    stack = stack1;
                    partial = vec![(obj2, pad)];
                }
                // In-degree 0, has out-properties: open new scopes, then
                // start a fresh partial from this object.
                (0, false) => {
                    stack = open(out_props, stack, partial);
                    partial = vec![(obj, pad)];
                }
                (_, false) => unreachable!("Invariant"),
            }
            i += 1;
        }
    }
    fn visit_fix<'b, 'a: 'b>(mem: &'b Bump, fix: &'a GraphFix<'a>) -> &'b RebuildFix<'b> {
        // Walk the fix chain, then rebuild the RebuildFix bottom-up.
        let mut recorded: Vec<(&'b Term<'b>, bool)> = Vec::new();
        let mut cur = fix;
        let last: &'b Term<'b> = loop {
            match cur {
                GraphFix::Last(term) => break map_term_chain(mem, *term),
                GraphFix::Next(term, fix1, pad) => {
                    recorded.push((map_term_chain(mem, *term), *pad));
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
    visit_doc(mem, doc)
}
