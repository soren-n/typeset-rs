//! Document rendering: [`Doc`] → `String`.
//!
//! Every traversal here is iterative. A [`Doc`] is a flat arena, so the
//! renderer walks it by arena id, keeping its descent state in heap-allocated
//! frame stacks instead of on the native stack, so arbitrarily deep documents
//! render with a constant native stack.
//!
//! Two traversals share the machinery: `render_obj` produces output and
//! commits pack marks; `fold` only measures, and undoes any marks it recorded
//! before returning so the caller's marks are untouched.

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
        if tab != 0 {
            self.lvl += tab - (self.lvl % tab);
        }
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

/// Frame for the measuring traversal ([`Renderer::fold`]): the remaining work
/// and any cursor field to restore once a child subtree has been folded.
enum MFrame {
    Obj(ObjId),
    RestoreLvl(usize),
    RestoreHead(bool),
    /// After the left of a `Comp`: pad, drop `head`, visit the right, then
    /// restore `head`.
    CompMid(ObjId, Pad),
}

/// Frame for the output traversal ([`Renderer::render_obj`]).
enum RFrame {
    Obj(ObjId),
    RestoreLvl(usize),
    RestoreBreak(bool),
    /// After the left of a `Comp`: decide the break, then render the right.
    CompMid(ObjId, Pad),
}

/// Outcome of resolving a `Pack` mark.
struct PackStep {
    /// Columns the pack advanced by (spaces to emit when producing output);
    /// `0` the first time a mark is seen.
    offset: usize,
    /// Whether this call recorded a new mark (measuring must undo it).
    fresh: bool,
}

struct Renderer<'a> {
    cfg: Config,
    doc: &'a Doc,
    /// The recorded column of each pack, by pack index; `None` until seen.
    marks: Vec<Option<usize>>,
    /// Frame stack and inserted-mark undo list for `fold`, and the frame
    /// stack for `render_obj`; owned here so every call reuses them.
    fold_stack: Vec<MFrame>,
    inserted: Vec<usize>,
    frames: Vec<RFrame>,
    out: String,
}

impl<'a> Renderer<'a> {
    fn new(doc: &'a Doc, cfg: Config) -> Self {
        Renderer {
            cfg,
            doc,
            marks: vec![None; doc.packs],
            fold_stack: Vec::new(),
            inserted: Vec::new(),
            frames: Vec::new(),
            // The output is at least the document's text; reserving it (plus a
            // newline per line) leaves only indentation to grow into.
            out: String::with_capacity(doc.text.len() + doc.lines.len()),
        }
    }

    fn push_spaces(&mut self, n: usize) {
        self.out.extend(std::iter::repeat_n(' ', n));
    }

    /// Resolves a `Pack` mark. The first time `index` is seen the current
    /// column is recorded and the level is lifted to it (no advance). On any
    /// later sighting the level is lifted to the recorded column and the
    /// cursor advances by the resulting offset.
    fn resolve_pack(&mut self, index: usize, cur: &mut Cursor) -> PackStep {
        match self.marks[index] {
            None => {
                self.marks[index] = Some(cur.pos);
                cur.lvl = max(cur.lvl, cur.pos);
                PackStep {
                    offset: 0,
                    fresh: true,
                }
            }
            Some(mark) => {
                cur.lvl = max(cur.lvl, mark);
                let offset = cur.offset();
                cur.advance(offset);
                PackStep {
                    offset,
                    fresh: false,
                }
            }
        }
    }

    /// Folds `obj` into its ending position without emitting output: where
    /// `obj` finishes if laid out from `cur`. Marks inserted while folding are
    /// undone before returning.
    ///
    /// This is the head-of-line slow path of [`will_fit`](Self::will_fit): at
    /// the head of a line `Nest`/`Pack` offsets depend on the live indentation
    /// level and pack marks, so the extent must be folded from the actual
    /// cursor. Width-bounded: the position only ever advances while measuring
    /// and the caller only compares the result against the width, so the fold
    /// stops as soon as the position passes it.
    fn fold(&mut self, obj: ObjId, mut cur: Cursor) -> usize {
        let Doc { objs, extents, .. } = self.doc;
        let mut stack = std::mem::take(&mut self.fold_stack);
        stack.clear();
        self.inserted.clear();
        stack.push(MFrame::Obj(obj));
        while let Some(frame) = stack.pop() {
            if cur.pos > self.cfg.width {
                break;
            }
            match frame {
                MFrame::Obj(o) => match &objs[o] {
                    ObjNode::Run(_) => cur.advance(extents[o]),
                    ObjNode::Grp(child) | ObjNode::Seq(child) => stack.push(MFrame::Obj(*child)),
                    ObjNode::Nest(child) => {
                        stack.push(MFrame::RestoreLvl(cur.lvl));
                        cur.indent(self.cfg.tab);
                        let offset = cur.offset();
                        cur.advance(offset);
                        stack.push(MFrame::Obj(*child));
                    }
                    ObjNode::Pack(index, child) => {
                        stack.push(MFrame::RestoreLvl(cur.lvl));
                        let index = *index as usize;
                        if self.resolve_pack(index, &mut cur).fresh {
                            self.inserted.push(index);
                        }
                        stack.push(MFrame::Obj(*child));
                    }
                    ObjNode::Comp(left, right, pad) => {
                        stack.push(MFrame::CompMid(*right, *pad));
                        stack.push(MFrame::Obj(*left));
                    }
                },
                MFrame::RestoreLvl(lvl) => cur.lvl = lvl,
                MFrame::RestoreHead(head) => cur.head = head,
                MFrame::CompMid(right, pad) => {
                    cur.advance(pad.width());
                    stack.push(MFrame::RestoreHead(cur.head));
                    cur.head = false;
                    stack.push(MFrame::Obj(right));
                }
            }
        }
        for index in self.inserted.drain(..) {
            self.marks[index] = None;
        }
        self.fold_stack = stack;
        cur.pos
    }

    /// Whether `obj` fits within the width if laid out from `cur`. Mid-line
    /// this is arithmetic on the precomputed extent (neither `Nest` nor `Pack`
    /// advances the position when `head` is false); only at the head of a
    /// line does it fold.
    fn will_fit(&mut self, obj: ObjId, cur: Cursor) -> bool {
        if !cur.head {
            return cur.pos.saturating_add(self.doc.extents[obj]) <= self.cfg.width;
        }
        self.fold(obj, cur) <= self.cfg.width
    }

    /// Whether the next composition boundary reachable from `obj` passes the
    /// width. Break decisions are made mid-line, where the precomputed
    /// boundary distance is exact.
    fn should_break(&self, obj: ObjId, cur: Cursor) -> bool {
        cur.broken || self.cfg.width < cur.pos.saturating_add(self.doc.next_comps[obj])
    }

    /// Renders one document object, threading the cursor. Marks inserted
    /// here are kept: they accumulate forward across the whole document.
    fn render_obj(&mut self, obj: ObjId, cur: &mut Cursor) {
        let Doc {
            objs,
            runs,
            text,
            extents,
            ..
        } = self.doc;
        let mut stack = std::mem::take(&mut self.frames);
        stack.clear();
        stack.push(RFrame::Obj(obj));
        while let Some(frame) = stack.pop() {
            match frame {
                RFrame::Obj(o) => match &objs[o] {
                    ObjNode::Run(range) => {
                        for run in range.slice(runs) {
                            self.push_spaces(run.pad.width());
                            self.out.push_str(run.text.slice(text));
                        }
                        cur.advance(extents[o]);
                    }
                    ObjNode::Grp(child) => {
                        stack.push(RFrame::RestoreBreak(cur.broken));
                        cur.broken = false;
                        stack.push(RFrame::Obj(*child));
                    }
                    ObjNode::Seq(child) => {
                        // A sequence that doesn't fit renders broken; either
                        // way the child renders next.
                        if !self.will_fit(*child, *cur) {
                            stack.push(RFrame::RestoreBreak(cur.broken));
                            cur.broken = true;
                        }
                        stack.push(RFrame::Obj(*child));
                    }
                    ObjNode::Nest(child) => {
                        stack.push(RFrame::RestoreLvl(cur.lvl));
                        cur.indent(self.cfg.tab);
                        let offset = cur.offset();
                        cur.advance(offset);
                        self.push_spaces(offset);
                        stack.push(RFrame::Obj(*child));
                    }
                    ObjNode::Pack(index, child) => {
                        stack.push(RFrame::RestoreLvl(cur.lvl));
                        let step = self.resolve_pack(*index as usize, cur);
                        self.push_spaces(step.offset);
                        stack.push(RFrame::Obj(*child));
                    }
                    ObjNode::Comp(left, right, pad) => {
                        stack.push(RFrame::CompMid(*right, *pad));
                        stack.push(RFrame::Obj(*left));
                    }
                },
                RFrame::RestoreLvl(lvl) => cur.lvl = lvl,
                RFrame::RestoreBreak(broken) => cur.broken = broken,
                RFrame::CompMid(right, pad) => {
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
                    stack.push(RFrame::Obj(right));
                }
            }
        }
        self.frames = stack;
    }

    /// Renders every line, joined by newlines. Pack marks and the indentation
    /// level carry across lines; the head flag, position and broken state
    /// reset per line.
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
    doc.render(tab, width)
}

impl Doc {
    /// Renders this document; see [`render`].
    pub fn render(&self, tab: usize, width: usize) -> String {
        Renderer::new(self, Config { width, tab }).render()
    }
}
