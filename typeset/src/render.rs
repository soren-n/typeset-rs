//! Document rendering: [`Doc`] → `String`.
//!
//! A [`Doc`] is a flat arena, so the renderer walks it by arena id, keeping
//! its descent state in a heap-allocated frame stack instead of on the native
//! stack: arbitrarily deep documents render with a constant native stack.
//!
//! Break decisions are arithmetic on the document's precomputed extent
//! tables. Mid-line, neither `Nest` nor `Pack` advances the position, so an
//! object's width is state-independent. At the head of a line the
//! indentation offsets depend on the live level and pack marks, but offsets
//! are only ever emitted before the first text on the line, so the head-of-
//! line measure walks the object's left spine and adds the flat extent.

use crate::doc::{Doc, ObjId, ObjNode};
use crate::layout::Pad;
use std::cmp::max;

/// Rendering parameters, fixed for the whole render.
#[derive(Copy, Clone)]
struct Config {
    /// Target line width; break decisions compare against it.
    width: usize,
    /// Columns per indentation level.
    tab: usize,
}

/// The layout position while rendering.
#[derive(Copy, Clone)]
struct Cursor {
    /// At the head of a line: nothing emitted on it yet, so `Nest`/`Pack`
    /// offsets still apply.
    head: bool,
    /// Inside a broken sequence: every composition breaks.
    broken: bool,
    /// Current indentation level, in columns.
    lvl: usize,
    /// Current column.
    pos: usize,
}

impl Cursor {
    const START: Cursor = Cursor {
        head: true,
        broken: false,
        lvl: 0,
        pos: 0,
    };

    fn advance(&mut self, columns: usize) {
        self.pos += columns;
    }

    /// Raises the indentation level to the next multiple of `tab`.
    fn indent(&mut self, tab: usize) {
        self.lvl = indent(self.lvl, tab);
    }

    /// Starts a new line within the same document object.
    fn newline(&mut self) {
        self.head = true;
        self.pos = 0;
    }

    /// Starts a new document line: also leaves any broken sequence.
    fn reset(&mut self) {
        self.head = true;
        self.broken = false;
        self.pos = 0;
    }

    /// Columns to emit to reach the indentation level: only at the head of a
    /// line, where the position never passes the level.
    fn offset(&self) -> usize {
        if !self.head {
            return 0;
        }
        debug_assert!(
            self.pos <= self.lvl,
            "head position {} past indentation level {}",
            self.pos,
            self.lvl
        );
        self.lvl - self.pos
    }
}

/// The next multiple of `tab` above `lvl` (`lvl` itself when `tab` is 0, so
/// a zero tab disables indentation).
fn indent(lvl: usize, tab: usize) -> usize {
    if tab == 0 { lvl } else { lvl + tab - lvl % tab }
}

/// Frame for the output traversal ([`Renderer::render_obj`]).
enum Frame {
    Obj(ObjId),
    RestoreLvl(usize),
    RestoreBreak(bool),
    /// After the left of a `Comp`: decide the break, then render the right.
    CompMid(ObjId, Pad),
}

struct Renderer<'a> {
    cfg: Config,
    doc: &'a Doc,
    /// The recorded column of each pack, by pack index; `None` until seen.
    marks: Vec<Option<usize>>,
    /// Frame stack for `render_obj`; owned here so every line reuses it.
    frames: Vec<Frame>,
    out: String,
}

impl<'a> Renderer<'a> {
    fn new(doc: &'a Doc, cfg: Config) -> Self {
        Renderer {
            cfg,
            doc,
            marks: vec![None; doc.packs],
            frames: Vec::new(),
            // The output is at least the document's text; reserving it (plus a
            // newline per line) leaves only indentation to grow into.
            out: String::with_capacity(doc.text.len() + doc.lines.len()),
        }
    }

    fn push_spaces(&mut self, n: usize) {
        self.out.extend(std::iter::repeat_n(' ', n));
    }

    /// Resolves a `Pack` mark while rendering, returning the columns the
    /// cursor advanced by. The first time `index` is seen the current column
    /// is recorded and the level is lifted to it (no advance). On any later
    /// sighting the level is lifted to the recorded column and the cursor
    /// advances by the resulting offset.
    fn resolve_pack(&mut self, index: usize, cur: &mut Cursor) -> usize {
        match self.marks[index] {
            None => {
                self.marks[index] = Some(cur.pos);
                cur.lvl = max(cur.lvl, cur.pos);
                0
            }
            Some(mark) => {
                cur.lvl = max(cur.lvl, mark);
                let offset = cur.offset();
                cur.advance(offset);
                offset
            }
        }
    }

    /// Where `obj` ends if laid out from `cur` at the head of a line.
    ///
    /// Only the wrappers on `obj`'s left spine — before its first text — can
    /// emit an indentation offset; everything after is mid-line, where the
    /// precomputed extent is exact. Each offset brings the position up to
    /// the level, so the spine walk tracks the level alone. A pack seen for
    /// the first time would only lift the level to the position, which is
    /// no change at the head of a line, and a measure never records marks.
    fn head_end(&self, obj: ObjId, cur: Cursor) -> usize {
        let Doc { objs, extents, .. } = self.doc;
        let mut lvl = cur.lvl;
        let mut pos = cur.pos;
        let mut o = obj;
        loop {
            o = match &objs[o] {
                ObjNode::Run(_) => break,
                ObjNode::Grp(child) | ObjNode::Seq(child) => *child,
                ObjNode::Comp(left, ..) => *left,
                ObjNode::Nest(child) => {
                    lvl = indent(lvl, self.cfg.tab);
                    pos = lvl;
                    *child
                }
                ObjNode::Pack(index, child) => {
                    if let Some(mark) = self.marks[*index as usize] {
                        lvl = max(lvl, mark);
                        pos = lvl;
                    }
                    *child
                }
            };
        }
        pos + extents[obj]
    }

    /// Whether `obj` fits within the width if laid out from `cur`.
    fn will_fit(&self, obj: ObjId, cur: Cursor) -> bool {
        let end = if cur.head {
            self.head_end(obj, cur)
        } else {
            cur.pos + self.doc.extents[obj]
        };
        end <= self.cfg.width
    }

    /// Whether the next composition boundary reachable from `obj` passes the
    /// width. Break decisions are made mid-line, where the precomputed
    /// boundary distance is exact.
    fn should_break(&self, obj: ObjId, cur: Cursor) -> bool {
        cur.broken || self.cfg.width < cur.pos + self.doc.next_comps[obj]
    }

    /// Renders one document object, threading the cursor. Marks recorded
    /// here are kept: they accumulate forward across the whole document.
    fn render_obj(&mut self, obj: ObjId, cur: &mut Cursor) {
        let Doc {
            objs,
            text,
            extents,
            ..
        } = self.doc;
        let mut stack = std::mem::take(&mut self.frames);
        stack.clear();
        stack.push(Frame::Obj(obj));
        while let Some(frame) = stack.pop() {
            match frame {
                Frame::Obj(o) => match &objs[o] {
                    ObjNode::Run(range) => {
                        self.out.push_str(range.slice(text));
                        cur.advance(extents[o]);
                    }
                    ObjNode::Grp(child) => {
                        stack.push(Frame::RestoreBreak(cur.broken));
                        cur.broken = false;
                        stack.push(Frame::Obj(*child));
                    }
                    ObjNode::Seq(child) => {
                        // A sequence that doesn't fit renders broken; either
                        // way the child renders next.
                        if !self.will_fit(*child, *cur) {
                            stack.push(Frame::RestoreBreak(cur.broken));
                            cur.broken = true;
                        }
                        stack.push(Frame::Obj(*child));
                    }
                    ObjNode::Nest(child) => {
                        stack.push(Frame::RestoreLvl(cur.lvl));
                        cur.indent(self.cfg.tab);
                        let offset = cur.offset();
                        cur.advance(offset);
                        self.push_spaces(offset);
                        stack.push(Frame::Obj(*child));
                    }
                    ObjNode::Pack(index, child) => {
                        stack.push(Frame::RestoreLvl(cur.lvl));
                        let offset = self.resolve_pack(*index as usize, cur);
                        self.push_spaces(offset);
                        stack.push(Frame::Obj(*child));
                    }
                    ObjNode::Comp(left, right, pad) => {
                        stack.push(Frame::CompMid(*right, *pad));
                        stack.push(Frame::Obj(*left));
                    }
                },
                Frame::RestoreLvl(lvl) => cur.lvl = lvl,
                Frame::RestoreBreak(broken) => cur.broken = broken,
                Frame::CompMid(right, pad) => {
                    // `cur` is the cursor left by the left operand. Decide from
                    // where the right operand would start if the line went on.
                    let mut joined = *cur;
                    joined.advance(pad.width());
                    joined.head = false;
                    if self.should_break(right, joined) {
                        cur.newline();
                        let offset = cur.offset();
                        cur.advance(offset);
                        self.out.push('\n');
                        self.push_spaces(offset);
                    } else {
                        self.push_spaces(pad.width());
                        *cur = joined;
                    }
                    stack.push(Frame::Obj(right));
                }
            }
        }
        self.frames = stack;
    }

    /// Renders every line, joined by newlines. Pack marks carry across
    /// lines; the head flag, position and broken state reset per line, and
    /// the indentation level is back to zero once a line's wrappers unwind.
    fn render(mut self) -> String {
        let mut cur = Cursor::START;
        for (i, line) in self.doc.lines.iter().enumerate() {
            if i > 0 {
                self.out.push('\n');
            }
            cur.reset();
            if let Some(obj) = line {
                self.render_obj(*obj, &mut cur);
            }
        }
        self.out
    }
}

impl Doc {
    /// Renders this document to a formatted string.
    ///
    /// Rendering only reads the document, so the same [`Doc`] can be rendered
    /// repeatedly (e.g. at several widths) without cloning or recompiling it.
    ///
    /// `tab` is the number of spaces per indentation level. `width` is the
    /// target line width, counted in `char`s (not display columns — East
    /// Asian wide characters and emoji count as one, so text using them
    /// renders wider than the requested width). It is a target, not a limit:
    /// a run wider than it still renders on one line. Use a very large width
    /// to disable wrapping.
    ///
    /// ```rust
    /// use typeset::*;
    ///
    /// let doc = pad(text("hello"), text("world")).compile();
    /// // Render at several widths without moving the document.
    /// assert_eq!(doc.render(2, 5), "hello\nworld");
    /// assert_eq!(doc.render(2, 80), "hello world");
    /// ```
    pub fn render(&self, tab: usize, width: usize) -> String {
        Renderer::new(self, Config { width, tab }).render()
    }
}
