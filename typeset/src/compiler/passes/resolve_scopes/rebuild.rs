//! Phase 3 of resolve_scopes: GraphDoc → RebuildDoc.
//!
//! Reads the solved scope graph per line and rebuilds an explicit composition
//! spine (grp/seq wrappers and left compositions) into the flat postorder
//! [`RebuildDoc`] arena. Node payloads and pads are read straight from the
//! borrowed `FixedDoc` line (nodes are index-aligned with the line's items).

use super::graph::{GraphDoc, GraphLine, NONE, Property};
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
// the RebuildCont closure stack used by `visit_line`), flattened into shared
// buffers so threading them allocates nothing per scope.
//
// A partial is a left composition spine, stored innermost-last (the tail is
// `[.., (xk,pk)]` with `(xk,pk)` innermost); `apply_rpartial` folds from the
// end, yielding `Comp(x0, Comp(x1, .. Comp(xk, obj, pk) .., p1), p0)`.
//
// A continuation is a stack of steps stored innermost-last; applying folds
// from the end. The stack of continuations is one flat step vector delimited
// by `bounds` (each entry the start of one continuation, top's start last).

/// A continuation step. A captured partial is a range into the shared
/// partials buffer.
#[derive(Debug, Copy, Clone)]
enum RStep {
    Grp,
    Seq,
    Partial(u32, u32),
}

/// The flat continuation state, reused across lines: cleared per line, so its
/// buffers amortize across the whole document.
struct ContState {
    /// All live continuations' steps, concatenated in stack order.
    steps: Vec<RStep>,
    /// Start index in `steps` of each continuation, top's last. The first
    /// entry is always 0: the line's identity continuation.
    bounds: Vec<u32>,
    /// Partial spines: captured regions below `cur_start` (addressed by
    /// `RStep::Partial` ranges), the live partial from `cur_start` on.
    partials: Vec<(RObjId, bool)>,
    /// Start of the live partial region in `partials`.
    cur_start: u32,
}

impl ContState {
    /// Resets to one empty identity continuation and an empty live partial.
    fn reset(&mut self) {
        self.steps.clear();
        self.bounds.clear();
        self.bounds.push(0);
        self.partials.clear();
        self.cur_start = 0;
    }
}

// Applies a partial spine to an object (innermost element first).
fn apply_rpartial(b: &mut Builder, partial: &[(RObjId, bool)], obj: RObjId) -> RObjId {
    let mut result = obj;
    for &(left, pad) in partial.iter().rev() {
        result = b.obj(RebuildObj::Comp(left, result, pad));
    }
    result
}

// Applies the steps from `start` to the end of the step vector to an object
// (innermost step first) and truncates them away — one continuation applied
// and popped, or the final identity continuation for `start` 0.
fn apply_steps(b: &mut Builder, st: &mut ContState, start: usize, obj: RObjId) -> RObjId {
    let mut result = obj;
    for i in (start..st.steps.len()).rev() {
        result = match st.steps[i] {
            RStep::Grp => b.obj(RebuildObj::Grp(result)),
            RStep::Seq => b.obj(RebuildObj::Seq(result)),
            RStep::Partial(s, e) => apply_rpartial(b, &st.partials[s as usize..e as usize], result),
        };
    }
    st.steps.truncate(start);
    result
}

// Pops `count` continuations, applying each to the accumulating object.
fn close(b: &mut Builder, st: &mut ContState, count: usize, term: RObjId) -> RObjId {
    let mut result = term;
    for _ in 0..count {
        let start = st.bounds.pop().expect("Invariant") as usize;
        result = apply_steps(b, st, start, result);
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
    let mut st = ContState {
        steps: Vec::new(),
        bounds: Vec::new(),
        partials: Vec::new(),
        cur_start: 0,
    };
    let lines: Vec<RObjId> = doc
        .lines
        .iter()
        .map(|line| visit_line(&mut b, doc, line, &mut st))
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

fn visit_line<'a>(
    b: &mut Builder<'a>,
    g: &GraphDoc<'_, 'a>,
    gl: &GraphLine<'_, 'a>,
    st: &mut ContState,
) -> RObjId {
    // Walk the nodes in order, threading the continuation stack and the live
    // partial spine. `line.seps[i].pad` is the pad between node `i` and
    // `i + 1`.
    st.reset();
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
            // In-degree 0, no out-properties: extend the live partial spine.
            (0, true) => st.partials.push((obj, pad)),
            // In-degree > 0, no out-properties: close the incoming scopes,
            // then start a fresh partial from the closed object.
            (_, true) => {
                let applied = apply_rpartial(b, &st.partials[st.cur_start as usize..], obj);
                let obj2 = close(b, st, in_deg, applied);
                st.partials.truncate(st.cur_start as usize);
                st.partials.push((obj2, pad));
            }
            // In-degree 0, has out-properties: capture the live partial onto
            // the top continuation, push a grp/seq continuation per out-edge
            // property (in list order), then start a fresh partial from this
            // object.
            (0, false) => {
                let end = st.partials.len() as u32;
                st.steps.push(RStep::Partial(st.cur_start, end));
                st.cur_start = end;
                let mut e = node.outs_head;
                while e != NONE {
                    st.bounds.push(st.steps.len() as u32);
                    st.steps.push(match g.edges[e as usize].prop {
                        Property::Grp => RStep::Grp,
                        Property::Seq => RStep::Seq,
                    });
                    e = g.edges[e as usize].next_out;
                }
                st.partials.push((obj, pad));
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
    let applied = apply_rpartial(b, &st.partials[st.cur_start as usize..], obj);
    let obj2 = close(b, st, last_node.ins_len as usize, applied);
    if st.bounds[..] != [0] {
        unreachable!("Invariant")
    }
    apply_steps(b, st, 0, obj2)
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
