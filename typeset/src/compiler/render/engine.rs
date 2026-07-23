//! Document rendering engine
//!
//! This module contains the rendering logic that transforms compiled Doc
//! structures into final string output with proper formatting and line breaking.
//!
//! Every traversal here is iterative. A [`Doc`] is a flat arena, so the renderer
//! walks it by arena index into the object/fixed-object node slices, never by
//! consuming or recursing down owning boxes. The descent state lives in
//! heap-allocated frame stacks (`Vec<...Frame>`) of indices instead of on the
//! native stack, so arbitrarily deep layouts render with a constant native stack.
//!
//! Pack marks live in a dense `Vec<usize>` threaded as `&mut`, indexed by pack
//! index (compilation assigns them as dense DFS counters; [`Doc::packs`] is the
//! slot count) with `NO_MARK` as the empty sentinel. In the real output pass
//! (`render_obj`) marks accumulate forward and are never rolled back. The
//! head-of-line measuring fold ([`will_fit`] via [`fold`]) must not leak its
//! marks into the caller, so it records the indices it inserts and clears them
//! before returning — measurement only ever inserts a mark when the slot is
//! empty, so clearing exactly those slots restores the caller's marks.

use crate::compiler::types::{Doc, FixId, FixNode, ObjId, ObjNode, Row, text_width};
use std::cmp::max;

/// Empty pack-mark slot. Marks record line positions, which are column counts
/// bounded far below `usize::MAX`.
const NO_MARK: usize = usize::MAX;

/// The node slices and extent tables of a [`Doc`], passed by value (a few
/// slice refs) so the traversals index objects, fixed objects, and precomputed
/// mid-line extents without carrying the whole document around.
#[derive(Copy, Clone)]
struct Arena<'a> {
    objs: &'a [ObjNode],
    fixes: &'a [FixNode],
    /// Per-object flat mid-line extent (see [`Doc::extents`]).
    extents: &'a [usize],
    /// Per-object mid-line advance to the first composition boundary.
    next_comps: &'a [usize],
}

impl<'a> Arena<'a> {
    fn obj(&self, id: ObjId) -> &'a ObjNode {
        &self.objs[id as usize]
    }

    fn fix(&self, id: FixId) -> &'a FixNode {
        &self.fixes[id as usize]
    }
}

#[derive(Debug, Copy, Clone)]
struct State {
    width: usize,
    tab: usize,
    head: bool,
    broken: bool,
    lvl: usize,
    pos: usize,
}

fn make_state(width: usize, tab: usize) -> State {
    State {
        width,
        tab,
        head: true,
        broken: false,
        lvl: 0,
        pos: 0,
    }
}

fn inc_pos(n: usize, state: State) -> State {
    State {
        pos: state.pos + n,
        ..state
    }
}

fn indent(tab: usize, state: State) -> State {
    if tab == 0 {
        return state;
    }
    let lvl = state.lvl;
    let lvl1 = lvl + (tab - (lvl % tab));
    State { lvl: lvl1, ..state }
}

fn newline(state: State) -> State {
    State {
        head: true,
        pos: 0,
        ..state
    }
}

fn reset(state: State) -> State {
    State {
        head: true,
        broken: false,
        pos: 0,
        ..state
    }
}

fn get_offset(state: State) -> usize {
    if !state.head {
        return 0;
    }
    // At the head of a line the position never passes the indentation level:
    // every head-path step either leaves `pos` alone or advances it exactly to
    // `lvl`. A violation would mean a real layout bug, so fail loudly instead
    // of clamping it away.
    debug_assert!(
        state.pos <= state.lvl,
        "head position {} past indentation level {}",
        state.pos,
        state.lvl
    );
    state.lvl - state.pos
}

/// Append `n` spaces to `result` (iterative `push_spaces`).
fn push_spaces(result: &mut String, n: usize) {
    result.extend(std::iter::repeat_n(' ', n));
}

/// Outcome of resolving a `Pack` mark at `index`, shared by every traversal.
struct PackStep {
    /// State to continue folding/rendering the pack's child from.
    state: State,
    /// Columns the pack advanced by (spaces to emit when producing output); `0`
    /// the first time a mark is seen.
    offset: usize,
    /// Whether this call recorded a new mark (the measuring passes must undo it).
    fresh: bool,
}

/// Resolves a `Pack` mark, threading `marks` uniformly across all traversals.
///
/// The first time `index` is seen the current column is recorded and the level
/// is lifted to it (no advance). On any later sighting the level is lifted to
/// the recorded column and the state advances by the resulting offset. Callers
/// diverge only in what they do with the result: output renders `offset` spaces
/// and keeps the mark; the measuring folds ignore `offset` and drop a `fresh`
/// mark before returning.
fn resolve_pack(marks: &mut [usize], index: usize, state: State) -> PackStep {
    let lvl = state.lvl;
    match marks[index] {
        NO_MARK => {
            let pos = state.pos;
            marks[index] = pos;
            PackStep {
                state: State {
                    lvl: max(lvl, pos),
                    ..state
                },
                offset: 0,
                fresh: true,
            }
        }
        lvl1 => {
            let state1 = State {
                lvl: max(lvl, lvl1),
                ..state
            };
            let offset = get_offset(state1);
            PackStep {
                state: inc_pos(offset, state1),
                offset,
                fresh: false,
            }
        }
    }
}

/// Frame for the state-only measuring traversal ([`fold`]).
///
/// The fold reduces a document object to a single position without producing
/// any output, so a frame only needs to describe the remaining work and any
/// state field to restore once a child subtree has been folded.
enum MFrame {
    Obj(ObjId),
    Fix(FixId),
    RestoreLvl(usize),
    RestoreHead(bool),
    /// After visiting the left of a `Comp`: pad, drop `head`, visit the right,
    /// then restore `head`.
    CompMid(ObjId, bool),
    /// After visiting the left of a fixed `Comp`: pad, then visit the right.
    FixCompMid(FixId, bool),
}

/// Reusable buffers for [`fold`]: its frame stack and its inserted-marks undo
/// list. The fold runs per head-of-line fit check, so the renderer owns one
/// set of buffers and every call reuses them instead of allocating.
#[derive(Default)]
struct Scratch {
    stack: Vec<MFrame>,
    inserted: Vec<usize>,
}

/// Folds `obj` into its ending position without emitting output (iterative):
/// where `obj` finishes if laid out from `state`. Any marks inserted while
/// folding are undone before returning, so `marks` is left exactly as the
/// caller passed it.
///
/// This is the head-of-line slow path of [`will_fit`]: at the head of a line
/// `Nest`/`Pack` offsets depend on the live indentation level and pack marks,
/// so the extent must be folded from the actual state. Mid-line the
/// precomputed [`Doc`] extent tables answer instead, and [`should_break`]
/// (always mid-line) never folds at all.
///
/// Width-bounded: the position only ever advances while measuring (nothing a
/// fold visits can move it backwards), and the one consumer ([`will_fit`])
/// only compares the result against the target width — so the fold stops as
/// soon as the position passes it, costing at most O(width) regardless of how
/// large the subtree is.
fn fold(
    arena: Arena,
    obj: ObjId,
    state: State,
    marks: &mut [usize],
    scratch: &mut Scratch,
) -> usize {
    let Scratch { stack, inserted } = scratch;
    stack.clear();
    inserted.clear();
    let mut st = state;
    stack.push(MFrame::Obj(obj));
    while let Some(frame) = stack.pop() {
        if st.pos > st.width {
            break;
        }
        match frame {
            MFrame::Obj(o) => match arena.obj(o) {
                ObjNode::Text(data) => st = inc_pos(text_width(data), st),
                ObjNode::Fix(fix) => stack.push(MFrame::Fix(*fix)),
                ObjNode::Grp(obj1) | ObjNode::Seq(obj1) => stack.push(MFrame::Obj(*obj1)),
                ObjNode::Nest(obj1) => {
                    let lvl = st.lvl;
                    let state1 = indent(st.tab, st);
                    let offset = get_offset(state1);
                    st = inc_pos(offset, state1);
                    stack.push(MFrame::RestoreLvl(lvl));
                    stack.push(MFrame::Obj(*obj1));
                }
                ObjNode::Pack(index, obj1) => {
                    let index = *index as usize;
                    let lvl = st.lvl;
                    let step = resolve_pack(marks, index, st);
                    st = step.state;
                    // Measuring must not leak marks: record the ones it created
                    // so they can be removed before returning.
                    if step.fresh {
                        inserted.push(index);
                    }
                    stack.push(MFrame::RestoreLvl(lvl));
                    stack.push(MFrame::Obj(*obj1));
                }
                // Fold both operands, padding and dropping `head` between them.
                ObjNode::Comp(left, right, pad) => {
                    stack.push(MFrame::CompMid(*right, *pad));
                    stack.push(MFrame::Obj(*left));
                }
            },
            MFrame::Fix(f) => match arena.fix(f) {
                FixNode::Text(data) => st = inc_pos(text_width(data), st),
                FixNode::Comp(left, right, pad) => {
                    stack.push(MFrame::FixCompMid(*right, *pad));
                    stack.push(MFrame::Fix(*left));
                }
            },
            MFrame::RestoreLvl(lvl) => st = State { lvl, ..st },
            MFrame::RestoreHead(head) => st = State { head, ..st },
            MFrame::CompMid(right, pad) => {
                st = inc_pos(usize::from(pad), st);
                let head = st.head;
                st = State { head: false, ..st };
                stack.push(MFrame::RestoreHead(head));
                stack.push(MFrame::Obj(right));
            }
            MFrame::FixCompMid(right, pad) => {
                st = inc_pos(usize::from(pad), st);
                stack.push(MFrame::Fix(right));
            }
        }
    }
    for index in inserted.drain(..) {
        marks[index] = NO_MARK;
    }
    st.pos
}

/// Whether `obj` fits within the width if laid out from `state`.
///
/// Mid-line this is pure arithmetic on the precomputed extent (neither `Nest`
/// nor `Pack` advances the position when `head` is false, so the flat extent
/// is exact); only at the head of a line — where indentation offsets depend on
/// the live state — does it fold.
fn will_fit(
    arena: Arena,
    obj: ObjId,
    state: State,
    marks: &mut [usize],
    scratch: &mut Scratch,
) -> bool {
    if !state.head {
        return state.pos.saturating_add(arena.extents[obj as usize]) <= state.width;
    }
    fold(arena, obj, state, marks, scratch) <= state.width
}

/// Whether the next composition boundary reachable from `obj` passes the
/// width. Break decisions are made mid-line (`head` is false), where the
/// precomputed boundary distance is exact — pure arithmetic, no traversal.
fn should_break(arena: Arena, obj: ObjId, state: State) -> bool {
    state.broken || state.width < state.pos.saturating_add(arena.next_comps[obj as usize])
}

/// Frame for the output-producing object traversal.
///
/// The current [`State`] and the growing output buffer are threaded as mutable
/// registers; a frame carries only the remaining work and the state field to
/// restore once a child subtree has been rendered.
enum RFrame {
    Obj(ObjId),
    Fix(FixId),
    RestoreLvl(usize),
    RestoreBreak(bool),
    /// After rendering the left of a `Comp`: decide the break, then render the
    /// right. `state` at this point is the state produced by the left.
    CompMid(ObjId, bool),
    /// After rendering the left of a fixed `Comp`: pad, then render the right.
    FixCompMid(FixId, bool),
}

/// Render one document object into `result`, threading `state` (iterative).
///
/// Unlike the measuring passes, marks inserted here are kept: they accumulate
/// forward across the whole document exactly as the recursive formulation
/// threaded them.
fn render_obj(
    arena: Arena,
    obj: ObjId,
    state: &mut State,
    marks: &mut [usize],
    scratch: &mut Scratch,
    result: &mut String,
) {
    let mut st = *state;
    let mut stack: Vec<RFrame> = vec![RFrame::Obj(obj)];
    while let Some(frame) = stack.pop() {
        match frame {
            RFrame::Obj(o) => match arena.obj(o) {
                ObjNode::Text(data) => {
                    st = inc_pos(text_width(data), st);
                    result.push_str(data);
                }
                ObjNode::Fix(fix) => stack.push(RFrame::Fix(*fix)),
                ObjNode::Grp(obj1) => {
                    stack.push(RFrame::RestoreBreak(st.broken));
                    st = State {
                        broken: false,
                        ..st
                    };
                    stack.push(RFrame::Obj(*obj1));
                }
                ObjNode::Seq(obj1) => {
                    // A sequence that doesn't fit renders broken; either way
                    // the child renders next.
                    if !will_fit(arena, *obj1, st, marks, scratch) {
                        stack.push(RFrame::RestoreBreak(st.broken));
                        st = State { broken: true, ..st };
                    }
                    stack.push(RFrame::Obj(*obj1));
                }
                ObjNode::Nest(obj1) => {
                    let lvl = st.lvl;
                    let state1 = indent(st.tab, st);
                    let offset = get_offset(state1);
                    st = inc_pos(offset, state1);
                    push_spaces(result, offset);
                    stack.push(RFrame::RestoreLvl(lvl));
                    stack.push(RFrame::Obj(*obj1));
                }
                ObjNode::Pack(index, obj1) => {
                    let index = *index as usize;
                    let lvl = st.lvl;
                    // Output keeps the mark (`fresh` is irrelevant here) and emits
                    // the advance; on a fresh mark the offset is 0, so this is a
                    // no-op push, exactly matching the measuring fold.
                    let step = resolve_pack(marks, index, st);
                    st = step.state;
                    push_spaces(result, step.offset);
                    stack.push(RFrame::RestoreLvl(lvl));
                    stack.push(RFrame::Obj(*obj1));
                }
                ObjNode::Comp(left, right, pad) => {
                    stack.push(RFrame::CompMid(*right, *pad));
                    stack.push(RFrame::Obj(*left));
                }
            },
            RFrame::Fix(f) => match arena.fix(f) {
                FixNode::Text(data) => {
                    st = inc_pos(text_width(data), st);
                    result.push_str(data);
                }
                FixNode::Comp(left, right, pad) => {
                    stack.push(RFrame::FixCompMid(*right, *pad));
                    stack.push(RFrame::Fix(*left));
                }
            },
            RFrame::RestoreLvl(lvl) => st = State { lvl, ..st },
            RFrame::RestoreBreak(broken) => st = State { broken, ..st },
            RFrame::CompMid(right, pad) => {
                // `st` is the state produced by the left operand.
                let state1 = st;
                let state3 = State {
                    head: false,
                    ..inc_pos(usize::from(pad), state1)
                };
                if should_break(arena, right, state3) {
                    let state2 = newline(state1);
                    let offset = get_offset(state2);
                    st = inc_pos(offset, state2);
                    result.push('\n');
                    push_spaces(result, offset);
                } else {
                    push_spaces(result, usize::from(pad));
                    st = state3;
                }
                stack.push(RFrame::Obj(right));
            }
            RFrame::FixCompMid(right, pad) => {
                let padding = usize::from(pad);
                push_spaces(result, padding);
                st = inc_pos(padding, st);
                stack.push(RFrame::Fix(right));
            }
        }
    }
    *state = st;
}

/// Renders a compiled document to a formatted string.
///
/// Rendering only reads the document, so the same [`Doc`] can be rendered
/// repeatedly (e.g. at several widths) without cloning or recompiling it.
///
/// `tab` is the number of spaces per indentation level. `width` is the target
/// line width, counted in `char`s (not display columns — East Asian wide
/// characters and emoji count as one, so text using them renders wider than the
/// requested width). Use a very large width (e.g. 10000) to disable wrapping.
///
/// # Examples
///
/// ```rust
/// use typeset::{compile, render, text, comp, Pad, Break};
///
/// let doc = compile(comp(
///     text("hello"),
///     text("world"),
///     Pad::Padded, Break::Breakable,
/// ));
/// // Render at several widths without moving the document.
/// assert!(render(&doc, 2, 5).contains('\n'));
/// assert_eq!(render(&doc, 2, 80), "hello world");
/// ```
pub fn render(doc: &Doc, tab: usize, width: usize) -> String {
    let arena = Arena {
        objs: doc.objs(),
        fixes: doc.fixes(),
        extents: doc.extents(),
        next_comps: doc.next_comps(),
    };
    let mut st = make_state(width, tab);
    let mut marks: Vec<usize> = vec![NO_MARK; doc.packs()];
    let mut scratch = Scratch::default();
    // The output is at least the document's text; reserving it (plus a
    // newline per row) leaves only indentation to grow into.
    let mut result = String::with_capacity(doc.text_bytes() + doc.rows().len());
    // The document spine is a linear `Vec<Row>` in document order, so it is
    // walked with a plain loop. `marks` and `lvl` survive `reset`, so they carry
    // across lines exactly as the recursive formulation threaded them. A `Line`
    // row (always last) ends the document; `Eod` is running off the end.
    for row in doc.rows() {
        st = reset(st);
        match row {
            Row::Empty => result.push('\n'),
            Row::Break(obj) => {
                render_obj(arena, *obj, &mut st, &mut marks, &mut scratch, &mut result);
                result.push('\n');
            }
            Row::Line(obj) => {
                render_obj(arena, *obj, &mut st, &mut marks, &mut scratch, &mut result);
                break;
            }
        }
    }
    result
}
