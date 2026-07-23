//! Phase 3 of resolve_scopes: GraphDoc → RebuildDoc.
//!
//! Reads the solved scope graph per line and rebuilds an explicit composition
//! spine (grp/seq wrappers and left compositions) into the flat postorder
//! [`RebuildDoc`] arena. Node payloads and pads are read straight from the
//! borrowed `FixedDoc` line (nodes are index-aligned with the line's items).

use super::graph::{GraphDoc, GraphLine, NONE, NodeData, Property};
use crate::compiler::types::{
    FixRun, FixedItem, RFixId, RObjId, RebuildDoc, RebuildFix, RebuildObj, push_node,
};

/// Appends arena nodes children-first while rebuilding, so a parent's child
/// indices always already exist.
struct Builder<'a> {
    objs: Vec<RebuildObj<'a>>,
    fixes: Vec<RebuildFix<'a>>,
}

impl<'a> Builder<'a> {
    fn obj(&mut self, node: RebuildObj<'a>) -> RObjId {
        push_node(&mut self.objs, node)
    }

    fn fix(&mut self, node: RebuildFix<'a>) -> RFixId {
        push_node(&mut self.fixes, node)
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

// Composes `partial` into the top continuation, then pushes a grp/seq
// continuation for each of the node's out-edge properties (in list order).
fn open(g: &GraphDoc<'_, '_>, node: &NodeData, stack: &mut RStack, partial: RPartial) {
    // Prepend a Partial step onto the current top continuation (a `push` in
    // the reversed-storage convention), then push a fresh single-step
    // continuation per property.
    stack
        .last_mut()
        .expect("Invariant")
        .push(RStep::Partial(partial));
    let mut e = node.outs_head;
    while e != NONE {
        stack.push(match g.edges[e as usize].prop {
            Property::Grp => vec![RStep::Grp],
            Property::Seq => vec![RStep::Seq],
        });
        e = g.edges[e as usize].next_out;
    }
}

// Pops `count` continuations, applying each to the accumulating object.
fn close(b: &mut Builder, count: usize, stack: &mut RStack, term: RObjId) -> RObjId {
    let mut result = term;
    for _ in 0..count {
        let top = stack.pop().expect("Invariant");
        result = apply_rcont(b, &top, result);
    }
    result
}

pub(super) fn rebuild<'a>(doc: &GraphDoc<'_, 'a>) -> RebuildDoc<'a> {
    // Every graph node yields at least one object, so the node total is a
    // capacity floor for the object arena.
    let mut b = Builder {
        objs: Vec::with_capacity(doc.nodes.len()),
        fixes: Vec::new(),
    };
    let lines: Vec<RObjId> = doc
        .lines
        .iter()
        .map(|line| visit_line(&mut b, doc, line))
        .collect();
    RebuildDoc {
        lines,
        objs: b.objs,
        fixes: b.fixes,
    }
}

/// Builds one graph node's payload (its line's like-indexed item) into the
/// arena.
fn visit_item<'a>(b: &mut Builder<'a>, item: &FixedItem<'a>) -> RObjId {
    match item {
        FixedItem::Fix(run) => {
            let fix1 = visit_fix(b, run);
            b.obj(RebuildObj::Fix(fix1))
        }
        FixedItem::Term(term) => b.obj(RebuildObj::Term(term)),
    }
}

fn visit_line<'a>(b: &mut Builder<'a>, g: &GraphDoc<'_, 'a>, gl: &GraphLine<'_, 'a>) -> RObjId {
    // Walk the nodes in order, threading the continuation stack and the left
    // composition spine (partial). `line.seps[i].pad` is the pad between node
    // `i` and `i + 1`. The initial stack holds one identity continuation.
    let mut stack: RStack = vec![Vec::new()];
    let mut partial: RPartial = Vec::new();
    let (last_item, rest) = gl
        .line
        .items
        .split_last()
        .expect("every line has at least one node");
    for (i, item) in rest.iter().enumerate() {
        let node = &g.nodes[gl.nodes_start as usize + i];
        let obj = visit_item(b, item);
        let in_deg = node.ins_len as usize;
        let pad = gl.line.seps[i].pad;
        match (in_deg, node.outs_head == NONE) {
            // In-degree 0, no out-properties: extend the partial spine.
            (0, true) => partial.push((obj, pad)),
            // In-degree > 0, no out-properties: close the incoming scopes,
            // then start a fresh partial from the closed object.
            (_, true) => {
                let applied = apply_rpartial(b, &partial, obj);
                let obj2 = close(b, in_deg, &mut stack, applied);
                partial = vec![(obj2, pad)];
            }
            // In-degree 0, has out-properties: open new scopes, then
            // start a fresh partial from this object.
            (0, false) => {
                open(g, node, &mut stack, std::mem::take(&mut partial));
                partial = vec![(obj, pad)];
            }
            (_, false) => unreachable!("Invariant"),
        }
    }
    // Final node of the line: it never has out-properties. Close any incoming
    // scopes, then apply the one remaining (identity) continuation.
    let last_node = &g.nodes[gl.nodes_end as usize - 1];
    assert!(
        last_node.outs_head == NONE,
        "Invariant: line ends without open scopes"
    );
    let obj = visit_item(b, last_item);
    let applied = apply_rpartial(b, &partial, obj);
    let obj2 = close(b, last_node.ins_len as usize, &mut stack, applied);
    let [cont] = &stack[..] else {
        unreachable!("Invariant")
    };
    apply_rcont(b, cont, obj2)
}

fn visit_fix<'a>(b: &mut Builder<'a>, run: &FixRun<'a>) -> RFixId {
    // Rebuild the run as a right-nested fixed composition spine. Terms pass
    // through by borrow; the pads are the run's separator pads.
    let last = *run.terms.last().expect("a fix run has at least one term");
    let mut rfix = b.fix(RebuildFix::Term(last));
    for k in (0..run.seps.len()).rev() {
        let left = b.fix(RebuildFix::Term(run.terms[k]));
        rfix = b.fix(RebuildFix::Comp(left, rfix, run.seps[k].pad));
    }
    rfix
}
