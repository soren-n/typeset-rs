//! Phase 3 of structurize: GraphDoc → RebuildDoc.
//!
//! Reads the solved scope graph per line and rebuilds an explicit composition
//! spine (grp/seq wrappers and left compositions) into the flat postorder
//! [`RebuildDoc`] arena.

use super::graph::{GraphDoc, GraphFix, GraphItem, GraphNode, NodeInfo, Property, graph_lines};
use crate::compiler::types::{RFixId, RObjId, RebuildDoc, RebuildFix, RebuildObj, Term};

/// Appends arena nodes children-first while rebuilding, so a parent's child
/// indices always already exist.
struct Builder<'a> {
    objs: Vec<RebuildObj<'a>>,
    fixes: Vec<RebuildFix<'a>>,
}

impl<'a> Builder<'a> {
    fn obj(&mut self, node: RebuildObj<'a>) -> RObjId {
        let id = self.objs.len() as RObjId;
        self.objs.push(node);
        id
    }

    fn fix(&mut self, node: RebuildFix<'a>) -> RFixId {
        let id = self.fixes.len() as RFixId;
        self.fixes.push(node);
        id
    }
}

// Defunctionalized rebuild continuations (replacing the `partial` closure and
// the RebuildCont closure stack used by `visit_line`).
//
// A partial is a left composition spine. It is stored so `push` appends the
// innermost (most recently added) element, i.e. the tail is `[.., (xk,pk)]` with
// `(xk,pk)` innermost; `apply_rpartial` folds from the end, yielding
// `Comp(x0, Comp(x1, .. Comp(xk, obj, pk) .., p1), p0)`.
type RPartial = Vec<(RObjId, bool)>;

// A continuation step. A continuation is a stack of steps stored so `push`
// appends the head (the step applied first / innermost); `apply_rcont` folds
// from the end. RStack is the stack of continuations, top at the end.
#[derive(Debug)]
enum RStep {
    Grp,
    Seq,
    Partial(RPartial),
}
type RCont = Vec<RStep>;
type RStack = Vec<RCont>;

// Applies a partial spine to an object (innermost element first).
fn apply_rpartial(b: &mut Builder, partial: &[(RObjId, bool)], obj: RObjId) -> RObjId {
    let mut result = obj;
    for &(left, pad) in partial.iter().rev() {
        result = b.obj(RebuildObj::Comp(left, result, pad));
    }
    result
}

// Applies a continuation (stack of steps) to an object (head step first).
fn apply_rcont(b: &mut Builder, cont: &[RStep], obj: RObjId) -> RObjId {
    let mut result = obj;
    for step in cont.iter().rev() {
        result = match step {
            RStep::Grp => b.obj(RebuildObj::Grp(result)),
            RStep::Seq => b.obj(RebuildObj::Seq(result)),
            RStep::Partial(partial) => apply_rpartial(b, partial, result),
        };
    }
    result
}

pub(super) fn rebuild<'a>(doc: &'a GraphDoc<'a>) -> RebuildDoc<'a> {
    // Per-node info in node-index order. The node items are read directly (not
    // copied): the term borrows flow straight through into the rebuilt objects.
    fn topology<'a>(nodes: &[&GraphNode<'a>]) -> Vec<NodeInfo<'a>> {
        fn num_ins(node: &GraphNode) -> u64 {
            let mut num = 0u64;
            let mut cur = node.ins_head.get();
            while let Some(edge) = cur {
                num += 1;
                cur = edge.ins_next.get();
            }
            num
        }
        fn prop_outs(node: &GraphNode) -> Vec<Property> {
            let mut props: Vec<Property> = Vec::new();
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
                item: node.item,
                in_degree: num_ins(node),
                outs: prop_outs(node),
            })
            .collect()
    }
    // Composes `partial` into the top continuation, then pushes a grp/seq
    // continuation for each property.
    fn open(props: &[Property], mut stack: RStack, partial: RPartial) -> RStack {
        // Prepend a Partial step onto the current top continuation (a `push`
        // in the reversed-storage convention), then push a fresh single-step
        // continuation per property.
        let mut top = stack.pop().expect("Invariant");
        top.push(RStep::Partial(partial));
        stack.push(top);
        for prop in props {
            stack.push(match prop {
                Property::Grp => vec![RStep::Grp],
                Property::Seq => vec![RStep::Seq],
            });
        }
        stack
    }
    // Pops `count` continuations, applying each to the accumulating object.
    fn close(b: &mut Builder, count: u64, mut stack: RStack, term: RObjId) -> (RStack, RObjId) {
        let mut result = term;
        for _ in 0..count {
            let top = stack.pop().expect("Invariant");
            result = apply_rcont(b, &top, result);
        }
        (stack, result)
    }
    fn finalize(b: &mut Builder, stack: RStack, term: RObjId) -> RObjId {
        match stack.as_slice() {
            [last] => apply_rcont(b, last, term),
            _ => unreachable!("Invariant"),
        }
    }
    fn visit_line<'a>(b: &mut Builder<'a>, info: &[NodeInfo<'a>], pads: &[bool]) -> RObjId {
        // Walk the per-node info, threading the continuation stack and the left
        // composition spine (partial). `pads` has one fewer element: `pads[i]`
        // is the pad between `info[i]` and `info[i + 1]`. The initial stack
        // holds one identity continuation (an empty step list).
        let mut stack: RStack = vec![Vec::new()];
        let mut partial: RPartial = Vec::new();
        let n = info.len();
        let mut i = 0;
        loop {
            let obj = match info[i].item {
                GraphItem::Fix(fix) => {
                    let fix1 = visit_fix(b, fix);
                    b.obj(RebuildObj::Fix(fix1))
                }
                GraphItem::Term(term) => b.obj(RebuildObj::Term(term)),
            };
            let in_deg = info[i].in_degree;
            let out_props = info[i].outs.as_slice();
            if i + 1 == n {
                // Final term of the line: it never has out-properties.
                if !out_props.is_empty() {
                    unreachable!("Invariant")
                }
                let applied = apply_rpartial(b, &partial, obj);
                // With no incoming scopes there is nothing to close; otherwise
                // close them first. Either way the line finalizes the same way.
                let (stack1, obj2) = if in_deg == 0 {
                    (stack, applied)
                } else {
                    close(b, in_deg, stack, applied)
                };
                return finalize(b, stack1, obj2);
            }
            let pad = pads[i];
            match (in_deg, out_props.is_empty()) {
                // In-degree 0, no out-properties: extend the partial spine.
                (0, true) => partial.push((obj, pad)),
                // In-degree > 0, no out-properties: close the incoming scopes,
                // then start a fresh partial from the closed object.
                (_, true) => {
                    let applied = apply_rpartial(b, &partial, obj);
                    let (stack1, obj2) = close(b, in_deg, stack, applied);
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
    fn visit_fix<'a>(b: &mut Builder<'a>, fix: &GraphFix<'a>) -> RFixId {
        // Walk the fix chain, then rebuild the RebuildFix bottom-up. Terms pass
        // through by borrow.
        let mut recorded: Vec<(&'a Term<'a>, bool)> = Vec::new();
        let mut cur = fix;
        let last = loop {
            match cur {
                GraphFix::Last(term) => break *term,
                GraphFix::Next(term, fix1, pad) => {
                    recorded.push((term, *pad));
                    cur = fix1;
                }
            }
        };
        let mut rfix = b.fix(RebuildFix::Term(last));
        for &(term, pad) in recorded.iter().rev() {
            let left = b.fix(RebuildFix::Term(term));
            rfix = b.fix(RebuildFix::Comp(left, rfix, pad));
        }
        rfix
    }

    let mut b = Builder {
        objs: Vec::new(),
        fixes: Vec::new(),
    };
    let mut lines: Vec<RObjId> = Vec::new();
    for (nodes, pads) in graph_lines(doc) {
        let info = topology(nodes);
        lines.push(visit_line(&mut b, &info, pads));
    }
    RebuildDoc {
        lines,
        objs: b.objs,
        fixes: b.fixes,
    }
}
