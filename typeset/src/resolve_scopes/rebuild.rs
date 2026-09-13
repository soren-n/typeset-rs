//! Phase 3 of resolve_scopes: GraphDoc → RebuildDoc.
//!
//! Reads the solved scope graph per line and rebuilds an explicit composition
//! spine (grp/seq wrappers and left compositions) into the flat postorder
//! [`RebuildDoc`] arena. Node payloads and pads are read straight from the
//! borrowed `FixedDoc` line (nodes are index-aligned with the line's items).

use super::graph::{GraphDoc, GraphLine};
use super::{Obj, ObjId as RObjId, RebuildDoc};
use crate::arena::{Arena, Range};
use crate::layout::Pad;
use crate::serialize::ScopeKind;

/// Appends arena nodes children-first while rebuilding, so a parent's child
/// ids always already exist.
struct Builder<'a> {
    objs: Arena<Obj<'a>>,
}

// Rebuild continuations, flattened into shared buffers so threading them
// allocates nothing per scope.
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
enum RStep<'a> {
    Grp,
    Seq,
    Partial(Range<(RObjId<'a>, Pad)>),
}

/// The flat continuation state, reused across lines: cleared per line, so its
/// buffers amortize across the whole document.
struct ContState<'a> {
    /// All live continuations' steps, concatenated in stack order.
    steps: Vec<RStep<'a>>,
    /// Start index in `steps` of each continuation, top's last. The first
    /// entry is always 0: the line's identity continuation.
    bounds: Vec<usize>,
    /// Partial spines: captured regions below `cur_start` (addressed by
    /// `RStep::Partial` ranges), the live partial from `cur_start` on.
    partials: Vec<(RObjId<'a>, Pad)>,
    /// Start of the live partial region in `partials`.
    cur_start: usize,
}

impl ContState<'_> {
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
fn apply_rpartial<'a>(
    b: &mut Builder<'a>,
    partial: &[(RObjId<'a>, Pad)],
    obj: RObjId<'a>,
) -> RObjId<'a> {
    let mut result = obj;
    for &(left, pad) in partial.iter().rev() {
        result = b.objs.push(Obj::Comp(left, result, pad));
    }
    result
}

// Applies the steps from `start` to the end of the step vector to an object
// (innermost step first) and truncates them away — one continuation applied
// and popped, or the final identity continuation for `start` 0.
fn apply_steps<'a>(
    b: &mut Builder<'a>,
    st: &mut ContState<'a>,
    start: usize,
    obj: RObjId<'a>,
) -> RObjId<'a> {
    let mut result = obj;
    for i in (start..st.steps.len()).rev() {
        result = match st.steps[i] {
            RStep::Grp => b.objs.push(Obj::Grp(result)),
            RStep::Seq => b.objs.push(Obj::Seq(result)),
            RStep::Partial(range) => apply_rpartial(b, range.slice(&st.partials), result),
        };
    }
    st.steps.truncate(start);
    result
}

// Pops `count` continuations, applying each to the accumulating object.
fn close<'a>(
    b: &mut Builder<'a>,
    st: &mut ContState<'a>,
    count: usize,
    term: RObjId<'a>,
) -> RObjId<'a> {
    let mut result = term;
    for _ in 0..count {
        let start = st.bounds.pop().expect("Invariant");
        result = apply_steps(b, st, start, result);
    }
    result
}

pub(super) fn rebuild<'a>(doc: &GraphDoc<'_, 'a>) -> RebuildDoc<'a> {
    // Every graph node yields at least one object, so the node total is a
    // capacity floor for the object arena.
    let mut b = Builder {
        objs: Arena::with_capacity(doc.nodes.len()),
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
    }
}

fn visit_line<'a>(
    b: &mut Builder<'a>,
    g: &GraphDoc<'_, 'a>,
    gl: &GraphLine<'a>,
    st: &mut ContState<'a>,
) -> RObjId<'a> {
    // Walk the nodes in order, threading the continuation stack and the live
    // partial spine. `line.seps[i].pad` is the pad between node `i` and
    // `i + 1`.
    st.reset();
    let items = gl.line.items.slice(&g.fixed.items);
    let seps = gl.line.seps.slice(&g.fixed.item_seps);
    let (last_item, rest) = items
        .split_last()
        .expect("every line has at least one node");
    for (i, item) in rest.iter().enumerate() {
        let node = &g.nodes[gl.nodes.id_at(i)];
        let obj = b.objs.push(Obj::Run(*item));
        let in_deg = node.ins_len as usize;
        let pad = seps[i].pad;
        match (in_deg, node.outs_head.is_none()) {
            // In-degree 0, no out-properties: extend the live partial spine.
            (0, true) => st.partials.push((obj, pad)),
            // In-degree > 0, no out-properties: close the incoming scopes,
            // then start a fresh partial from the closed object.
            (_, true) => {
                let applied = apply_rpartial(b, &st.partials[st.cur_start..], obj);
                let obj2 = close(b, st, in_deg, applied);
                st.partials.truncate(st.cur_start);
                st.partials.push((obj2, pad));
            }
            // In-degree 0, has out-properties: capture the live partial onto
            // the top continuation, push a grp/seq continuation per out-edge
            // property (in list order), then start a fresh partial from this
            // object.
            (0, false) => {
                let end = st.partials.len();
                st.steps.push(RStep::Partial(Range::new(st.cur_start, end)));
                st.cur_start = end;
                let mut e = node.outs_head;
                while let Some(edge) = e {
                    st.bounds.push(st.steps.len());
                    st.steps.push(match g.edges[edge].kind {
                        ScopeKind::Grp => RStep::Grp,
                        ScopeKind::Seq => RStep::Seq,
                    });
                    e = g.edges[edge].next_out;
                }
                st.partials.push((obj, pad));
            }
            (_, false) => unreachable!("Invariant"),
        }
    }
    // Final node of the line: it never has out-properties. Close any incoming
    // scopes, then apply the one remaining (identity) continuation.
    let last_node = &g.nodes[gl.nodes.id_at(gl.nodes.len() - 1)];
    assert!(
        last_node.outs_head.is_none(),
        "Invariant: line ends without open scopes"
    );
    let obj = b.objs.push(Obj::Run(*last_item));
    let applied = apply_rpartial(b, &st.partials[st.cur_start..], obj);
    let obj2 = close(b, st, last_node.ins_len as usize, applied);
    if st.bounds[..] != [0] {
        unreachable!("Invariant")
    }
    apply_steps(b, st, 0, obj2)
}
