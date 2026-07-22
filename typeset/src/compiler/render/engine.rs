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
//! Pack marks are held in a plain owned [`HashMap`] threaded as `&mut`. The
//! renderer only ever looks a mark up by index or inserts one, never iterating
//! in key order, so an unordered map is the right fit. In the real output pass
//! (`render_obj`) marks accumulate forward and are never rolled back. The
//! look-ahead measuring passes (`measure`/`next_comp`) must not leak their
//! marks into the caller, so each records the indices it inserts and removes
//! them before returning — measurement only ever inserts a mark when the index
//! is absent, so removing exactly those keys restores the caller's map.

use crate::compiler::types::{Doc, FixId, FixNode, ObjId, ObjNode, Row};
use std::cmp::max;
use std::collections::HashMap;

/// The two node slices of a [`Doc`], passed by value (a pair of slice refs) so
/// the traversals index objects and fixed objects without carrying the whole
/// document around.
#[derive(Copy, Clone)]
struct Arena<'a> {
    objs: &'a [ObjNode],
    fixes: &'a [FixNode],
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

/// Width of a literal in columns.
///
/// `String::len` is the UTF-8 byte length, which over-measures any non-ASCII
/// text and breaks lines far earlier than the requested width. Layout positions
/// are column counts, so count characters instead.
fn text_width(data: &str) -> usize {
    data.chars().count()
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
fn resolve_pack(marks: &mut HashMap<usize, usize>, index: usize, state: State) -> PackStep {
    let lvl = state.lvl;
    match marks.get(&index) {
        None => {
            let pos = state.pos;
            marks.insert(index, pos);
            PackStep {
                state: State {
                    lvl: max(lvl, pos),
                    ..state
                },
                offset: 0,
                fresh: true,
            }
        }
        Some(&lvl1) => {
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

/// Frame for the state-only measuring traversals (`measure`, `next_comp`).
///
/// These fold a document object into a single position without producing any
/// output, so a frame only needs to describe the remaining work and any state
/// field to restore once a child subtree has been folded.
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

/// Which look-ahead the state-only fold performs. The two modes share an
/// identical traversal and diverge at exactly two nodes — `Grp` and `Comp`.
#[derive(Debug, Copy, Clone, PartialEq)]
enum Fold {
    /// Full extent: fold the entire object, visiting both sides of every `Comp`.
    Measure,
    /// Stop at the first composition boundary: visit only the left of a `Comp`,
    /// and treat a non-`head` `Grp` as an opaque already-measured block.
    NextComp,
}

/// Folds `obj` into a single ending position without emitting output (iterative).
///
/// [`Fold::Measure`] returns where `obj` finishes if laid out from `state`;
/// [`Fold::NextComp`] returns the position of the next composition boundary
/// reachable from `obj`. Either way, any marks inserted while folding are undone
/// before returning, so `marks` is left exactly as the caller passed it.
///
/// Width-bounded: the position only ever advances while measuring (nothing a
/// fold visits can move it backwards), and both consumers ([`will_fit`],
/// [`should_break`]) only compare the result against the target width — so the
/// fold stops as soon as the position passes it. This keeps the repeated
/// look-aheads cheap: a comp decision costs at most O(width) regardless of how
/// large the subtree beyond it is.
fn fold(
    mode: Fold,
    arena: Arena,
    obj: ObjId,
    state: State,
    marks: &mut HashMap<usize, usize>,
) -> usize {
    let mut st = state;
    let mut inserted: Vec<usize> = Vec::new();
    let mut stack: Vec<MFrame> = vec![MFrame::Obj(obj)];
    while let Some(frame) = stack.pop() {
        if st.pos > st.width {
            break;
        }
        match frame {
            MFrame::Obj(o) => match arena.obj(o) {
                ObjNode::Text(data) => st = inc_pos(text_width(data), st),
                ObjNode::Fix(fix) => stack.push(MFrame::Fix(*fix)),
                // The first point of divergence: `Measure` always descends;
                // `NextComp` folds an already-laid-out group as one opaque block.
                ObjNode::Grp(obj1) => {
                    // Golden path: descend into the group. Only a non-head group
                    // under `NextComp` is folded inline as an opaque, already
                    // laid-out block.
                    if mode != Fold::NextComp || st.head {
                        stack.push(MFrame::Obj(*obj1));
                    } else {
                        let end = fold(Fold::Measure, arena, *obj1, st, marks);
                        st = State { pos: end, ..st };
                    }
                }
                ObjNode::Seq(obj1) => stack.push(MFrame::Obj(*obj1)),
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
                // The second point of divergence: `Measure` folds both operands
                // (padding and dropping `head` between them); `NextComp` stops at
                // the boundary, folding only the left operand.
                ObjNode::Comp(left, right, pad) => {
                    if mode == Fold::Measure {
                        stack.push(MFrame::CompMid(*right, *pad));
                    }
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
    for index in inserted {
        marks.remove(&index);
    }
    st.pos
}

/// Position at which `obj` finishes if laid out from `state`.
fn measure(arena: Arena, obj: ObjId, state: State, marks: &mut HashMap<usize, usize>) -> usize {
    fold(Fold::Measure, arena, obj, state, marks)
}

/// Position of the next composition boundary reachable from `obj`.
fn next_comp(arena: Arena, obj: ObjId, state: State, marks: &mut HashMap<usize, usize>) -> usize {
    fold(Fold::NextComp, arena, obj, state, marks)
}

fn will_fit(arena: Arena, obj: ObjId, state: State, marks: &mut HashMap<usize, usize>) -> bool {
    measure(arena, obj, state, marks) <= state.width
}

fn should_break(arena: Arena, obj: ObjId, state: State, marks: &mut HashMap<usize, usize>) -> bool {
    state.broken || state.width < next_comp(arena, obj, state, marks)
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
    marks: &mut HashMap<usize, usize>,
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
                    if !will_fit(arena, *obj1, st, marks) {
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
                if should_break(arena, right, state3, marks) {
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
    };
    let mut st = make_state(width, tab);
    let mut marks: HashMap<usize, usize> = HashMap::new();
    let mut result = String::new();
    // The document spine is a linear `Vec<Row>` in document order, so it is
    // walked with a plain loop. `marks` and `lvl` survive `reset`, so they carry
    // across lines exactly as the recursive formulation threaded them. A `Line`
    // row (always last) ends the document; `Eod` is running off the end.
    for row in doc.rows() {
        st = reset(st);
        match row {
            Row::Empty => result.push('\n'),
            Row::Break(obj) => {
                render_obj(arena, *obj, &mut st, &mut marks, &mut result);
                result.push('\n');
            }
            Row::Line(obj) => {
                render_obj(arena, *obj, &mut st, &mut marks, &mut result);
                break;
            }
        }
    }
    result
}
