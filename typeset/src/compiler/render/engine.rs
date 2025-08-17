//! Document rendering engine
//!
//! This module contains the rendering logic that transforms compiled Doc
//! structures into final string output with proper formatting and line breaking.

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

/// Renders a compiled document into a formatted string
///
/// # Arguments
/// * `doc` - The compiled document to render
/// * `tab` - Tab size for indentation
/// * `width` - Target line width for formatting
///
/// # Returns
/// A formatted string representation of the document
pub fn render(doc: Box<Doc>, tab: usize, width: usize) -> String {
    fn _whitespace(n: usize) -> String {
        " ".repeat(n)
    }
    fn _pad(n: usize, result: String) -> String {
        result + &_whitespace(n)
    }
    fn _measure<'b, 'a: 'b>(mem: &'b Bump, obj: &DocObj, state: State<'a>) -> usize {
        fn _visit_obj<'b, 'a: 'b>(mem: &'b Bump, obj: &DocObj, state: State<'a>) -> State<'b> {
            match obj {
                DocObj::Text(data) => _inc_pos(data.len(), state),
                DocObj::Fix(fix) => _visit_fix(fix, state),
                DocObj::Grp(obj1) => _visit_obj(mem, obj1, state),
                DocObj::Seq(obj1) => _visit_obj(mem, obj1, state),
                DocObj::Nest(obj1) => {
                    let lvl = state.lvl;
                    let state1 = _indent(state.tab, state);
                    let offset = _get_offset(state1);
                    let state2 = _inc_pos(offset, state1);
                    let state3 = _visit_obj(mem, obj1, state2);
                    State { lvl, ..state3 }
                }
                DocObj::Pack(index, obj1) => {
                    let index = *index as usize;
                    let lvl = state.lvl;
                    let marks = state.marks;
                    match marks.lookup(&total, index) {
                        None => {
                            let pos = state.pos;
                            let marks1 = marks.insert(mem, &total, index, pos);
                            let state1 = State {
                                marks: marks1,
                                ..state
                            };
                            let state2 = State {
                                lvl: max(lvl, pos),
                                ..state1
                            };
                            let state3 = _visit_obj(mem, obj1, state2);
                            State { lvl, ..state3 }
                        }
                        Some(lvl1) => {
                            let state1 = State {
                                lvl: max(lvl, lvl1),
                                ..state
                            };
                            let offset = _get_offset(state1);
                            let state2 = _inc_pos(offset, state1);
                            let state3 = _visit_obj(mem, obj1, state2);
                            State { lvl, ..state3 }
                        }
                    }
                }
                DocObj::Comp(left, right, pad) => {
                    let state1 = _visit_obj(mem, left, state);
                    let state2 = _inc_pos(if *pad { 1 } else { 0 }, state1);
                    let head = state2.head;
                    let state3 = State {
                        head: false,
                        ..state2
                    };
                    let state4 = _visit_obj(mem, right, state3);
                    State { head, ..state4 }
                }
            }
        }
        fn _visit_fix<'b, 'a: 'b>(fix: &DocObjFix, state: State<'a>) -> State<'a> {
            match fix {
                DocObjFix::Text(data) => _inc_pos(data.len(), state),
                DocObjFix::Comp(left, right, pad) => {
                    let state1 = _visit_fix(left, state);
                    let state2 = _inc_pos(if *pad { 1 } else { 0 }, state1);
                    _visit_fix(right, state2)
                }
            }
        }
        let state1 = _visit_obj(mem, obj, state);
        state1.pos
    }
    fn _next_comp<'b, 'a: 'b>(mem: &'b Bump, obj: &DocObj, state: State<'a>) -> usize {
        fn _visit_obj<'b, 'a: 'b>(mem: &'b Bump, obj: &DocObj, state: State<'a>) -> State<'b> {
            match obj {
                DocObj::Text(data) => _inc_pos(data.len(), state),
                DocObj::Fix(fix) => _visit_fix(mem, fix, state),
                DocObj::Grp(obj1) => {
                    let head = state.head;
                    if head {
                        _visit_obj(mem, obj1, state)
                    } else {
                        let obj_end_pos = _measure(mem, obj1, state);
                        State {
                            pos: obj_end_pos,
                            ..state
                        }
                    }
                }
                DocObj::Seq(obj1) => _visit_obj(mem, obj1, state),
                DocObj::Nest(obj1) => {
                    let lvl = state.lvl;
                    let state1 = _indent(state.tab, state);
                    let offset = _get_offset(state1);
                    let state2 = _inc_pos(offset, state1);
                    let state3 = _visit_obj(mem, obj1, state2);
                    State { lvl, ..state3 }
                }
                DocObj::Pack(index, obj1) => {
                    let index = *index as usize;
                    let lvl = state.lvl;
                    let marks = state.marks;
                    match marks.lookup(&total, index) {
                        None => {
                            let pos = state.pos;
                            let marks1 = marks.insert(mem, &total, index, pos);
                            let state1 = State {
                                marks: marks1,
                                ..state
                            };
                            let state2 = State {
                                lvl: max(lvl, pos),
                                ..state1
                            };
                            let state3 = _visit_obj(mem, obj1, state2);
                            State { lvl, ..state3 }
                        }
                        Some(lvl1) => {
                            let state1 = State {
                                lvl: max(lvl, lvl1),
                                ..state
                            };
                            let offset = _get_offset(state1);
                            let state2 = _inc_pos(offset, state1);
                            let state3 = _visit_obj(mem, obj1, state2);
                            State { lvl, ..state3 }
                        }
                    }
                }
                DocObj::Comp(left, _right, _pad) => _visit_obj(mem, left, state),
            }
        }
        #[allow(clippy::only_used_in_recursion)]
        fn _visit_fix<'b, 'a: 'b>(_mem: &'b Bump, fix: &DocObjFix, state: State<'a>) -> State<'a> {
            match fix {
                DocObjFix::Text(data) => _inc_pos(data.len(), state),
                DocObjFix::Comp(left, right, pad) => {
                    let state1 = _visit_fix(_mem, left, state);
                    let state2 = _inc_pos(if *pad { 1 } else { 0 }, state1);
                    _visit_fix(_mem, right, state2)
                }
            }
        }
        let state1 = _visit_obj(mem, obj, state);
        state1.pos
    }
    fn _will_fit<'b, 'a: 'b>(mem: &'b Bump, obj: &DocObj, state: State) -> bool {
        let obj_end_pos = _measure(mem, obj, state);
        obj_end_pos <= state.width
    }
    fn _should_break<'b, 'a: 'b>(mem: &'b Bump, obj: &DocObj, state: State) -> bool {
        let broken = state.broken;
        if broken {
            true
        } else {
            let next_comp_pos = _next_comp(mem, obj, state);
            state.width < next_comp_pos
        }
    }
    #[allow(clippy::boxed_local)]
    fn _visit_doc<'b, 'a: 'b>(
        mem: &'b Bump,
        doc: Box<Doc>,
        state: State<'a>,
    ) -> (State<'b>, String) {
        let state1 = _reset(state);
        match *doc {
            Doc::Eod => (state1, "".to_string()),
            Doc::Empty(doc1) => {
                let (state2, doc2) = _visit_doc(mem, doc1, state1);
                (state2, format!("\n{}", doc2))
            }
            Doc::Break(obj, doc1) => {
                let (state2, obj1) = _visit_obj(mem, *obj, state1, "".to_string());
                let state3 = _reset(state2);
                let (state4, doc2) = _visit_doc(mem, doc1, state3);
                (state4, format!("{}\n{}", obj1, doc2))
            }
            Doc::Line(obj) => _visit_obj(mem, *obj, state1, "".to_string()),
        }
    }
    fn _visit_obj<'b, 'a: 'b>(
        mem: &'b Bump,
        obj: DocObj,
        state: State<'a>,
        result: String,
    ) -> (State<'b>, String) {
        match obj {
            DocObj::Text(data) => {
                let state1 = _inc_pos(data.len(), state);
                (state1, result.clone() + &data)
            }
            DocObj::Fix(fix) => _visit_fix(mem, *fix, state, result),
            DocObj::Grp(obj1) => {
                let broken = state.broken;
                let state1 = State {
                    broken: false,
                    ..state
                };
                let (state2, result1) = _visit_obj(mem, *obj1, state1, result.clone());
                let state3 = State { broken, ..state2 };
                (state3, result1.clone())
            }
            DocObj::Seq(obj1) => {
                if _will_fit(mem, &obj1, state) {
                    _visit_obj(mem, *obj1, state, result)
                } else {
                    let broken = state.broken;
                    let state1 = State {
                        broken: true,
                        ..state
                    };
                    let (state2, result1) = _visit_obj(mem, *obj1, state1, result.clone());
                    let state3 = State { broken, ..state2 };
                    (state3, result1.clone())
                }
            }
            DocObj::Nest(obj1) => {
                let lvl = state.lvl;
                let state1 = _indent(state.tab, state);
                let offset = _get_offset(state1);
                let state2 = _inc_pos(offset, state1);
                let result1 = _pad(offset, result.clone());
                let (state3, result2) = _visit_obj(mem, *obj1, state2, result1.clone());
                let state4 = State { lvl, ..state3 };
                (state4, result2.clone())
            }
            DocObj::Pack(index, obj1) => {
                let index = index as usize;
                let lvl = state.lvl;
                let marks = state.marks;
                match marks.lookup(&total, index) {
                    None => {
                        let pos = state.pos;
                        let marks1 = marks.insert(mem, &total, index, pos);
                        let state1 = State {
                            marks: marks1,
                            ..state
                        };
                        let state2 = State {
                            lvl: max(lvl, pos),
                            ..state1
                        };
                        let (state3, result1) = _visit_obj(mem, *obj1, state2, result.clone());
                        let state4 = State { lvl, ..state3 };
                        (state4, result1.clone())
                    }
                    Some(lvl1) => {
                        let state1 = State {
                            lvl: max(lvl, lvl1),
                            ..state
                        };
                        let offset = _get_offset(state1);
                        let state2 = _inc_pos(offset, state1);
                        let result1 = _pad(offset, result.clone());
                        let (state3, result2) = _visit_obj(mem, *obj1, state2, result1.clone());
                        let state4 = State { lvl, ..state3 };
                        (state4, result2.clone())
                    }
                }
            }
            DocObj::Comp(left, right, pad) => {
                let (state1, result1) = _visit_obj(mem, *left, state, result);
                let state2 = _inc_pos(if pad { 1 } else { 0 }, state1);
                let state3 = State {
                    head: false,
                    ..state2
                };
                if _should_break(mem, &right, state3) {
                    let state2 = _newline(state1);
                    let offset = _get_offset(state2);
                    let state3 = _inc_pos(offset, state2);
                    let result2 = _pad(offset, result1.clone() + "\n");
                    _visit_obj(mem, *right, state3, result2)
                } else {
                    let result2 = _pad(if pad { 1 } else { 0 }, result1.clone());
                    _visit_obj(mem, *right, state3, result2)
                }
            }
        }
    }
    #[allow(clippy::only_used_in_recursion)]
    fn _visit_fix<'b, 'a: 'b>(
        _mem: &'b Bump,
        fix: DocObjFix,
        state: State<'a>,
        result: String,
    ) -> (State<'a>, String) {
        match fix {
            DocObjFix::Text(data) => {
                let state1 = _inc_pos(data.len(), state);
                (state1, result.clone() + &data)
            }
            DocObjFix::Comp(left, right, pad) => {
                let (state1, result1) = _visit_fix(_mem, *left, state, result);
                let padding = if pad { 1 } else { 0 };
                let result2 = _pad(padding, result1);
                let state2 = _inc_pos(padding, state1);
                _visit_fix(_mem, *right, state2, result2.clone())
            }
        }
    }
    let mem = Bump::new();
    let (_state, result) = _visit_doc(&mem, doc, _make_state(&mem, width, tab));
    result
}
