//! Document rendering engine
//!
//! This module contains the rendering logic that transforms compiled Doc
//! structures into final string output with proper formatting and line breaking.
//!
//! Every traversal here is iterative. The document is walked by borrowing
//! (`&Doc`), never by consuming it, so arbitrarily deep layouts render with a
//! constant native stack — the descent state lives in heap-allocated frame
//! stacks (`Vec<...Frame>`) instead of stack frames. Borrowing rather than
//! moving is also what lets [`Doc`](crate::compiler::types::Doc) carry an
//! iterative `Drop`.

use crate::compiler::types::{Doc, DocObj, DocObjFix};
use crate::{map::Map, order::total};
use bumpalo::Bump;
use std::cmp::max;

#[derive(Debug, Copy, Clone)]
struct State<'a> {
    width: usize,
    tab: usize,
    head: bool,
    broken: bool,
    lvl: usize,
    pos: usize,
    marks: &'a Map<'a, usize, usize>,
}

fn _make_state<'a>(mem: &'a Bump, width: usize, tab: usize) -> State<'a> {
    use crate::map;
    State {
        width,
        tab,
        head: true,
        broken: false,
        lvl: 0,
        pos: 0,
        marks: map::empty(mem),
    }
}

/// Width of a literal in columns.
///
/// `String::len` is the UTF-8 byte length, which over-measures any non-ASCII
/// text and breaks lines far earlier than the requested width. Layout positions
/// are column counts, so count characters instead.
fn _text_width(data: &str) -> usize {
    data.chars().count()
}

fn _inc_pos<'a>(n: usize, state: State<'a>) -> State<'a> {
    State {
        pos: state.pos + n,
        ..state
    }
}

fn _indent<'a>(tab: usize, state: State<'a>) -> State<'a> {
    if tab == 0 {
        state
    } else {
        let lvl = state.lvl;
        let lvl1 = lvl + (tab - (lvl % tab));
        State { lvl: lvl1, ..state }
    }
}

fn _newline<'a>(state: State<'a>) -> State<'a> {
    State {
        head: true,
        pos: 0,
        ..state
    }
}

fn _reset<'a>(state: State<'a>) -> State<'a> {
    State {
        head: true,
        broken: false,
        pos: 0,
        ..state
    }
}

fn _get_offset<'a>(state: State<'a>) -> usize {
    if !state.head {
        0
    } else {
        max(0, state.lvl - state.pos)
    }
}

/// Append `n` spaces to `result` (iterative `_pad`).
fn _pad(result: &mut String, n: usize) {
    result.extend(std::iter::repeat_n(' ', n));
}

/// Frame for the state-only measuring traversals (`_measure`, `_next_comp`).
///
/// These fold a document object into a single position without producing any
/// output, so a frame only needs to describe the remaining work and any state
/// field to restore once a child subtree has been folded.
enum MFrame<'t> {
    Obj(&'t DocObj),
    Fix(&'t DocObjFix),
    RestoreLvl(usize),
    RestoreHead(bool),
    /// After visiting the left of a `Comp`: pad, drop `head`, visit the right,
    /// then restore `head`.
    CompMid(&'t DocObj, bool),
    /// After visiting the left of a fixed `Comp`: pad, then visit the right.
    FixCompMid(&'t DocObjFix, bool),
}

/// Position at which `obj` finishes if laid out from `state` (iterative fold).
fn _measure<'t, 'b>(mem: &'b Bump, obj: &'t DocObj, state: State<'b>) -> usize {
    let mut st: State<'b> = state;
    let mut stack: Vec<MFrame<'t>> = vec![MFrame::Obj(obj)];
    while let Some(frame) = stack.pop() {
        match frame {
            MFrame::Obj(o) => match o {
                DocObj::Text(data) => st = _inc_pos(_text_width(data), st),
                DocObj::Fix(fix) => stack.push(MFrame::Fix(fix)),
                DocObj::Grp(obj1) => stack.push(MFrame::Obj(obj1)),
                DocObj::Seq(obj1) => stack.push(MFrame::Obj(obj1)),
                DocObj::Nest(obj1) => {
                    let lvl = st.lvl;
                    let state1 = _indent(st.tab, st);
                    let offset = _get_offset(state1);
                    st = _inc_pos(offset, state1);
                    stack.push(MFrame::RestoreLvl(lvl));
                    stack.push(MFrame::Obj(obj1));
                }
                DocObj::Pack(index, obj1) => {
                    let index = *index as usize;
                    let lvl = st.lvl;
                    let marks = st.marks;
                    match marks.lookup(&total, index) {
                        None => {
                            let pos = st.pos;
                            let marks1 = marks.insert(mem, &total, index, pos);
                            st = State {
                                marks: marks1,
                                lvl: max(lvl, pos),
                                ..st
                            };
                        }
                        Some(lvl1) => {
                            let state1 = State {
                                lvl: max(lvl, lvl1),
                                ..st
                            };
                            let offset = _get_offset(state1);
                            st = _inc_pos(offset, state1);
                        }
                    }
                    stack.push(MFrame::RestoreLvl(lvl));
                    stack.push(MFrame::Obj(obj1));
                }
                DocObj::Comp(left, right, pad) => {
                    stack.push(MFrame::CompMid(right, *pad));
                    stack.push(MFrame::Obj(left));
                }
            },
            MFrame::Fix(f) => match f {
                DocObjFix::Text(data) => st = _inc_pos(_text_width(data), st),
                DocObjFix::Comp(left, right, pad) => {
                    stack.push(MFrame::FixCompMid(right, *pad));
                    stack.push(MFrame::Fix(left));
                }
            },
            MFrame::RestoreLvl(lvl) => st = State { lvl, ..st },
            MFrame::RestoreHead(head) => st = State { head, ..st },
            MFrame::CompMid(right, pad) => {
                st = _inc_pos(if pad { 1 } else { 0 }, st);
                let head = st.head;
                st = State { head: false, ..st };
                stack.push(MFrame::RestoreHead(head));
                stack.push(MFrame::Obj(right));
            }
            MFrame::FixCompMid(right, pad) => {
                st = _inc_pos(if pad { 1 } else { 0 }, st);
                stack.push(MFrame::Fix(right));
            }
        }
    }
    st.pos
}

/// Position of the next composition boundary reachable from `obj` (iterative).
fn _next_comp<'t, 'b>(mem: &'b Bump, obj: &'t DocObj, state: State<'b>) -> usize {
    let mut st: State<'b> = state;
    let mut stack: Vec<MFrame<'t>> = vec![MFrame::Obj(obj)];
    while let Some(frame) = stack.pop() {
        match frame {
            MFrame::Obj(o) => match o {
                DocObj::Text(data) => st = _inc_pos(_text_width(data), st),
                DocObj::Fix(fix) => stack.push(MFrame::Fix(fix)),
                DocObj::Grp(obj1) => {
                    if st.head {
                        stack.push(MFrame::Obj(obj1));
                    } else {
                        let end = _measure(mem, obj1, st);
                        st = State { pos: end, ..st };
                    }
                }
                DocObj::Seq(obj1) => stack.push(MFrame::Obj(obj1)),
                DocObj::Nest(obj1) => {
                    let lvl = st.lvl;
                    let state1 = _indent(st.tab, st);
                    let offset = _get_offset(state1);
                    st = _inc_pos(offset, state1);
                    stack.push(MFrame::RestoreLvl(lvl));
                    stack.push(MFrame::Obj(obj1));
                }
                DocObj::Pack(index, obj1) => {
                    let index = *index as usize;
                    let lvl = st.lvl;
                    let marks = st.marks;
                    match marks.lookup(&total, index) {
                        None => {
                            let pos = st.pos;
                            let marks1 = marks.insert(mem, &total, index, pos);
                            st = State {
                                marks: marks1,
                                lvl: max(lvl, pos),
                                ..st
                            };
                        }
                        Some(lvl1) => {
                            let state1 = State {
                                lvl: max(lvl, lvl1),
                                ..st
                            };
                            let offset = _get_offset(state1);
                            st = _inc_pos(offset, state1);
                        }
                    }
                    stack.push(MFrame::RestoreLvl(lvl));
                    stack.push(MFrame::Obj(obj1));
                }
                DocObj::Comp(left, _right, _pad) => stack.push(MFrame::Obj(left)),
            },
            MFrame::Fix(f) => match f {
                DocObjFix::Text(data) => st = _inc_pos(_text_width(data), st),
                DocObjFix::Comp(left, right, pad) => {
                    stack.push(MFrame::FixCompMid(right, *pad));
                    stack.push(MFrame::Fix(left));
                }
            },
            MFrame::RestoreLvl(lvl) => st = State { lvl, ..st },
            MFrame::FixCompMid(right, pad) => {
                st = _inc_pos(if pad { 1 } else { 0 }, st);
                stack.push(MFrame::Fix(right));
            }
            // `_next_comp` visits only the left of a `Comp` and never touches
            // `head`, so it never pushes these; they are reachable only from
            // `_measure`.
            MFrame::CompMid(..) | MFrame::RestoreHead(_) => {
                unreachable!("_next_comp never pushes CompMid/RestoreHead")
            }
        }
    }
    st.pos
}

fn _will_fit<'b>(mem: &'b Bump, obj: &DocObj, state: State<'b>) -> bool {
    _measure(mem, obj, state) <= state.width
}

fn _should_break<'b>(mem: &'b Bump, obj: &DocObj, state: State<'b>) -> bool {
    if state.broken {
        true
    } else {
        state.width < _next_comp(mem, obj, state)
    }
}

/// Frame for the output-producing object traversal.
///
/// The current [`State`] and the growing output buffer are threaded as mutable
/// registers; a frame carries only the remaining work and the state field to
/// restore once a child subtree has been rendered.
enum RFrame<'t> {
    Obj(&'t DocObj),
    Fix(&'t DocObjFix),
    RestoreLvl(usize),
    RestoreBreak(bool),
    /// After rendering the left of a `Comp`: decide the break, then render the
    /// right. `state` at this point is the state produced by the left.
    CompMid(&'t DocObj, bool),
    /// After rendering the left of a fixed `Comp`: pad, then render the right.
    FixCompMid(&'t DocObjFix, bool),
}

/// Render one document object into `result`, threading `state` (iterative).
fn _render_obj<'t, 'b>(mem: &'b Bump, obj: &'t DocObj, state: &mut State<'b>, result: &mut String) {
    let mut st: State<'b> = *state;
    let mut stack: Vec<RFrame<'t>> = vec![RFrame::Obj(obj)];
    while let Some(frame) = stack.pop() {
        match frame {
            RFrame::Obj(o) => match o {
                DocObj::Text(data) => {
                    st = _inc_pos(_text_width(data), st);
                    result.push_str(data);
                }
                DocObj::Fix(fix) => stack.push(RFrame::Fix(fix)),
                DocObj::Grp(obj1) => {
                    let broken = st.broken;
                    st = State {
                        broken: false,
                        ..st
                    };
                    stack.push(RFrame::RestoreBreak(broken));
                    stack.push(RFrame::Obj(obj1));
                }
                DocObj::Seq(obj1) => {
                    if _will_fit(mem, obj1, st) {
                        stack.push(RFrame::Obj(obj1));
                    } else {
                        let broken = st.broken;
                        st = State { broken: true, ..st };
                        stack.push(RFrame::RestoreBreak(broken));
                        stack.push(RFrame::Obj(obj1));
                    }
                }
                DocObj::Nest(obj1) => {
                    let lvl = st.lvl;
                    let state1 = _indent(st.tab, st);
                    let offset = _get_offset(state1);
                    st = _inc_pos(offset, state1);
                    _pad(result, offset);
                    stack.push(RFrame::RestoreLvl(lvl));
                    stack.push(RFrame::Obj(obj1));
                }
                DocObj::Pack(index, obj1) => {
                    let index = *index as usize;
                    let lvl = st.lvl;
                    let marks = st.marks;
                    match marks.lookup(&total, index) {
                        None => {
                            let pos = st.pos;
                            let marks1 = marks.insert(mem, &total, index, pos);
                            st = State {
                                marks: marks1,
                                lvl: max(lvl, pos),
                                ..st
                            };
                        }
                        Some(lvl1) => {
                            let state1 = State {
                                lvl: max(lvl, lvl1),
                                ..st
                            };
                            let offset = _get_offset(state1);
                            st = _inc_pos(offset, state1);
                            _pad(result, offset);
                        }
                    }
                    stack.push(RFrame::RestoreLvl(lvl));
                    stack.push(RFrame::Obj(obj1));
                }
                DocObj::Comp(left, right, pad) => {
                    stack.push(RFrame::CompMid(right, *pad));
                    stack.push(RFrame::Obj(left));
                }
            },
            RFrame::Fix(f) => match f {
                DocObjFix::Text(data) => {
                    st = _inc_pos(_text_width(data), st);
                    result.push_str(data);
                }
                DocObjFix::Comp(left, right, pad) => {
                    stack.push(RFrame::FixCompMid(right, *pad));
                    stack.push(RFrame::Fix(left));
                }
            },
            RFrame::RestoreLvl(lvl) => st = State { lvl, ..st },
            RFrame::RestoreBreak(broken) => st = State { broken, ..st },
            RFrame::CompMid(right, pad) => {
                // `st` is the state produced by the left operand.
                let state1 = st;
                let state3 = State {
                    head: false,
                    .._inc_pos(if pad { 1 } else { 0 }, state1)
                };
                if _should_break(mem, right, state3) {
                    let state2 = _newline(state1);
                    let offset = _get_offset(state2);
                    st = _inc_pos(offset, state2);
                    result.push('\n');
                    _pad(result, offset);
                } else {
                    _pad(result, if pad { 1 } else { 0 });
                    st = state3;
                }
                stack.push(RFrame::Obj(right));
            }
            RFrame::FixCompMid(right, pad) => {
                let padding = if pad { 1 } else { 0 };
                _pad(result, padding);
                st = _inc_pos(padding, st);
                stack.push(RFrame::Fix(right));
            }
        }
    }
    *state = st;
}

/// Renders a compiled document into a formatted string, borrowing it.
///
/// The renderer only reads the document, so this is the primary entry point: it
/// takes `&Doc` and can be called repeatedly on the same document (e.g. to
/// render at several widths) without cloning it.
///
/// # Arguments
/// * `doc` - The compiled document to render
/// * `tab` - Tab size for indentation
/// * `width` - Target line width for formatting
///
/// # Returns
/// A formatted string representation of the document
pub fn render_ref(doc: &Doc, tab: usize, width: usize) -> String {
    let mem = Bump::new();
    let mut st = _make_state(&mem, width, tab);
    let mut result = String::new();
    // The document spine (`Empty` / `Break` / `Line` / `Eod`) is a linear list,
    // so it is walked with a plain loop rather than recursion. `marks` and `lvl`
    // survive `_reset`, so they carry across lines exactly as the recursive
    // formulation threaded them.
    let mut node: &Doc = doc;
    loop {
        st = _reset(st);
        match node {
            Doc::Eod => break,
            Doc::Empty(doc1) => {
                result.push('\n');
                node = doc1;
            }
            Doc::Break(obj, doc1) => {
                _render_obj(&mem, obj, &mut st, &mut result);
                result.push('\n');
                node = doc1;
            }
            Doc::Line(obj) => {
                _render_obj(&mem, obj, &mut st, &mut result);
                break;
            }
        }
    }
    result
}

/// Renders a compiled document into a formatted string, consuming it.
///
/// Convenience wrapper over [`render_ref`] for the common case of rendering a
/// document once. When rendering the same document more than once, prefer
/// [`render_ref`] to avoid moving (or cloning) it.
///
/// # Arguments
/// * `doc` - The compiled document to render
/// * `tab` - Tab size for indentation
/// * `width` - Target line width for formatting
///
/// # Returns
/// A formatted string representation of the document
pub fn render(doc: Box<Doc>, tab: usize, width: usize) -> String {
    render_ref(&doc, tab, width)
}
