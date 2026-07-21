//! Pass 1: Layout → Edsl (collapse broken sequences)
//!
//! This pass transforms Layout into Edsl by tracking and collapsing
//! broken sequences. It identifies compositions that will need to be
//! broken into newlines and marks them appropriately.
//!
//! Both stages are defunctionalized: rather than recursing on the native
//! stack (which aborts the process — not a catchable panic — at a few
//! hundred nesting levels), each stage walks the tree with an explicit
//! heap-allocated frame stack and a descend/ascend trampoline.

use crate::compiler::types::{Attr, Broken, Edsl, Layout};
use bumpalo::Bump;
use std::mem;

/// Transforms Layout into Edsl by collapsing broken sequences
pub fn broken<'b, 'a: 'b>(mem: &'b Bump, layout: Box<Layout>) -> &'b Edsl<'b> {
    let layout1 = mark(mem, layout);
    remove(mem, layout1, false)
}

/// Frames for the `mark` fold (Layout → (bool, Broken), bottom-up).
///
/// The result carried on ascent is `(bool, &Broken)`: the boolean records
/// whether the subtree contains a hard line break. Unary frames rebuild the
/// wrapper; `LineLeft`/`CompLeft` still hold the un-visited right child.
enum MarkFrame<'b> {
    Fix,
    Grp,
    Seq,
    Nest,
    Pack,
    LineLeft(Box<Layout>),
    LineRight(&'b Broken<'b>),
    CompLeft(Box<Layout>, Attr),
    CompRight(bool, &'b Broken<'b>, Attr),
}

/// Marks broken sequences: folds Layout into Broken, propagating a "contains a
/// line break" flag up so `Seq` nodes can record whether they must break.
fn mark<'b>(mem: &'b Bump, layout: Box<Layout>) -> &'b Broken<'b> {
    // Either a finished leaf value, or the next child to descend into. Computed
    // while borrowing `*cur`; acting on it happens after the borrow ends, so the
    // emptied box can be reassigned (and dropped) without a borrow conflict.
    // `Layout` carries an iterative `Drop`, so it cannot be destructured by
    // value — children are taken out with `mem::take` (leaving a `Null`).
    enum Step<'b> {
        Leaf(bool, &'b Broken<'b>),
        Descend(Box<Layout>),
    }
    let mut stack: Vec<MarkFrame<'b>> = Vec::new();
    let mut cur = layout;
    'descend: loop {
        // Descend `cur` until we reach a leaf, producing an ascending value.
        let step = match &mut *cur {
            Layout::Null => Step::Leaf(false, mem.alloc(Broken::Null)),
            Layout::Text(data) => {
                let data1 = mem.alloc_str(data.as_str());
                Step::Leaf(false, mem.alloc(Broken::Text(data1)))
            }
            Layout::Fix(layout1) => {
                stack.push(MarkFrame::Fix);
                Step::Descend(mem::take(layout1))
            }
            Layout::Grp(layout1) => {
                stack.push(MarkFrame::Grp);
                Step::Descend(mem::take(layout1))
            }
            Layout::Seq(layout1) => {
                stack.push(MarkFrame::Seq);
                Step::Descend(mem::take(layout1))
            }
            Layout::Nest(layout1) => {
                stack.push(MarkFrame::Nest);
                Step::Descend(mem::take(layout1))
            }
            Layout::Pack(layout1) => {
                stack.push(MarkFrame::Pack);
                Step::Descend(mem::take(layout1))
            }
            Layout::Line(left, right) => {
                stack.push(MarkFrame::LineLeft(mem::take(right)));
                Step::Descend(mem::take(left))
            }
            Layout::Comp(left, right, attr) => {
                let attr = *attr;
                stack.push(MarkFrame::CompLeft(mem::take(right), attr));
                Step::Descend(mem::take(left))
            }
        };
        let mut val: (bool, &'b Broken<'b>) = match step {
            Step::Descend(next) => {
                cur = next;
                continue 'descend;
            }
            Step::Leaf(broken, node) => (broken, node),
        };
        // Ascend: apply pending frames to `val` until we must descend again.
        loop {
            match stack.pop() {
                None => return val.1,
                Some(MarkFrame::Fix) => val = (val.0, mem.alloc(Broken::Fix(val.1))),
                Some(MarkFrame::Grp) => val = (val.0, mem.alloc(Broken::Grp(val.1))),
                Some(MarkFrame::Seq) => val = (val.0, mem.alloc(Broken::Seq(val.0, val.1))),
                Some(MarkFrame::Nest) => val = (val.0, mem.alloc(Broken::Nest(val.1))),
                Some(MarkFrame::Pack) => val = (val.0, mem.alloc(Broken::Pack(val.1))),
                Some(MarkFrame::LineLeft(right)) => {
                    stack.push(MarkFrame::LineRight(val.1));
                    cur = right;
                    continue 'descend;
                }
                Some(MarkFrame::LineRight(left1)) => {
                    val = (true, mem.alloc(Broken::Line(left1, val.1)));
                }
                Some(MarkFrame::CompLeft(right, attr)) => {
                    stack.push(MarkFrame::CompRight(val.0, val.1, attr));
                    cur = right;
                    continue 'descend;
                }
                Some(MarkFrame::CompRight(l_broken, left1, attr)) => {
                    val = (
                        l_broken || val.0,
                        mem.alloc(Broken::Comp(left1, val.1, attr)),
                    );
                }
            }
        }
    }
}

/// Frames for the `remove` pass (Broken → Edsl, CPS).
///
/// This is the defunctionalized form of the continuation chain the original
/// built with `compose`. The value carried on ascent is `&Edsl`. `LineLeft`
/// and `CompLeft` also carry the `broken` flag so the right subtree descends
/// with the same flag the parent received.
enum RemoveFrame<'b> {
    Fix,
    Grp,
    Seq,
    Nest,
    Pack,
    LineLeft {
        right: &'b Broken<'b>,
        broken: bool,
    },
    LineRight {
        left1: &'b Edsl<'b>,
    },
    CompLeft {
        right: &'b Broken<'b>,
        broken: bool,
        attr: Attr,
    },
    CompRight {
        left1: &'b Edsl<'b>,
        broken: bool,
        attr: Attr,
    },
}

/// Removes broken sequences: rewrites `Broken` into `Edsl`, turning broken
/// compositions into hard lines and dropping the `Seq` wrapper where the
/// sequence has already broken.
fn remove<'b>(mem: &'b Bump, layout: &'b Broken<'b>, broken: bool) -> &'b Edsl<'b> {
    let mut stack: Vec<RemoveFrame<'b>> = Vec::new();
    let mut cur = layout;
    let mut brk = broken;
    'descend: loop {
        let mut val: &'b Edsl<'b> = match cur {
            Broken::Null => mem.alloc(Edsl::Null),
            Broken::Text(data) => mem.alloc(Edsl::Text(data)),
            Broken::Fix(layout1) => {
                stack.push(RemoveFrame::Fix);
                cur = layout1;
                brk = false;
                continue 'descend;
            }
            Broken::Grp(layout1) => {
                stack.push(RemoveFrame::Grp);
                cur = layout1;
                brk = false;
                continue 'descend;
            }
            Broken::Seq(broken1, layout1) => {
                if *broken1 {
                    // Already broken: drop the Seq wrapper, descend broken.
                    cur = layout1;
                    brk = true;
                    continue 'descend;
                } else {
                    stack.push(RemoveFrame::Seq);
                    cur = layout1;
                    brk = false;
                    continue 'descend;
                }
            }
            Broken::Nest(layout1) => {
                stack.push(RemoveFrame::Nest);
                cur = layout1;
                continue 'descend;
            }
            Broken::Pack(layout1) => {
                stack.push(RemoveFrame::Pack);
                cur = layout1;
                continue 'descend;
            }
            Broken::Line(left, right) => {
                stack.push(RemoveFrame::LineLeft { right, broken: brk });
                cur = left;
                continue 'descend;
            }
            Broken::Comp(left, right, attr) => {
                stack.push(RemoveFrame::CompLeft {
                    right,
                    broken: brk,
                    attr: *attr,
                });
                cur = left;
                continue 'descend;
            }
        };
        loop {
            match stack.pop() {
                None => return val,
                Some(RemoveFrame::Fix) => val = mem.alloc(Edsl::Fix(val)),
                Some(RemoveFrame::Grp) => val = mem.alloc(Edsl::Grp(val)),
                Some(RemoveFrame::Seq) => val = mem.alloc(Edsl::Seq(val)),
                Some(RemoveFrame::Nest) => val = mem.alloc(Edsl::Nest(val)),
                Some(RemoveFrame::Pack) => val = mem.alloc(Edsl::Pack(val)),
                Some(RemoveFrame::LineLeft { right, broken: b }) => {
                    stack.push(RemoveFrame::LineRight { left1: val });
                    cur = right;
                    brk = b;
                    continue 'descend;
                }
                Some(RemoveFrame::LineRight { left1 }) => val = mem.alloc(Edsl::Line(left1, val)),
                Some(RemoveFrame::CompLeft {
                    right,
                    broken: b,
                    attr,
                }) => {
                    stack.push(RemoveFrame::CompRight {
                        left1: val,
                        broken: b,
                        attr,
                    });
                    cur = right;
                    brk = b;
                    continue 'descend;
                }
                Some(RemoveFrame::CompRight {
                    left1,
                    broken: b,
                    attr,
                }) => {
                    val = if b && !attr.fix {
                        mem.alloc(Edsl::Line(left1, val))
                    } else {
                        mem.alloc(Edsl::Comp(left1, val, attr))
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A depth that a native-stack recursion cannot survive: the recursive
    /// version of this pass aborts (stack overflow, not a catchable panic)
    /// around 400 nested compositions on a 2 MB stack. Reaching this depth
    /// without aborting proves the pass runs iteratively.
    const DEEP: usize = 50_000;

    fn deep_comp(depth: usize) -> Box<Layout> {
        let mut layout = Box::new(Layout::Text("x".to_string()));
        for _ in 0..depth {
            layout = Box::new(Layout::Comp(
                layout,
                Box::new(Layout::Text("y".to_string())),
                Attr {
                    pad: false,
                    fix: false,
                },
            ));
        }
        layout
    }

    #[test]
    fn broken_handles_deep_layout_without_overflow() {
        let mem = Bump::new();
        let edsl = broken(&mem, deep_comp(DEEP));
        // Walk the result iteratively to confirm the whole spine was built.
        let mut count = 0usize;
        let mut cur = edsl;
        while let Edsl::Comp(left, _right, _attr) = cur {
            count += 1;
            cur = left;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn broken_handles_deep_nest_without_overflow() {
        // Nest exercises the flag-preserving descent path.
        let mem = Bump::new();
        let mut layout = Box::new(Layout::Text("x".to_string()));
        for _ in 0..DEEP {
            layout = Box::new(Layout::Nest(layout));
        }
        let edsl = broken(&mem, layout);
        let mut count = 0usize;
        let mut cur = edsl;
        while let Edsl::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }
}
