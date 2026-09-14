//! The public output type: a [`Doc`] stored as a flat arena.
//!
//! The object graph (the `Comp`/`Grp`/`Nest`/… nodes) is one flat arena with
//! children referenced by arena id; a run of unbreakable text is a range into
//! the one `String` all text is concatenated in. The spine is one optional
//! root object per line in document order (`None` for an empty line). Each
//! object carries its mid-line measure, computed as it is pushed, for the
//! renderer's O(1) break decisions.
//!
//! Being flat, dropping or cloning a `Doc` touches a few `Vec`s of shallow
//! records and one `String`, so `Clone` and `Drop` derive and never recurse
//! no matter how deeply nested the document is; `Debug` walks the arena
//! with an explicit stack.

use crate::arena::{Arena, Id, Range};
use crate::dsl::{self, Binary, Shape, Unary};
use crate::layout::{Break, Pad};
use std::fmt;

pub(crate) type ObjId = Id<Obj>;

/// A node of the object graph. Children are arena ids, a run is a range
/// into the text buffer, so a node is a shallow record.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ObjNode {
    /// Text that never breaks: one or more literals, with the space of a
    /// padded fixed composition between them, already joined.
    Run(Range<str>),
    Grp(ObjId),
    Seq(ObjId),
    Nest(ObjId),
    Pack(u32, ObjId),
    Comp(ObjId, ObjId, Pad),
}

/// An object of the arena: its node and its precomputed mid-line extents,
/// which the renderer's break decisions read in O(1). Mid-line (not at the
/// head of a line) neither `Nest` nor `Pack` advances the position — their
/// offsets only apply at the head of a line — so both extents are exact,
/// state-independent sums over the children, which always precede their
/// parent in the arena.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Obj {
    pub(crate) node: ObjNode,
    /// How many columns the object advances when laid out mid-line.
    pub(crate) extent: usize,
    /// The advance to the first composition boundary: the sum along the left
    /// spine, resolving a group as one already-laid-out block (its full
    /// extent).
    pub(crate) next_comp: usize,
}

/// Width of a literal in columns.
///
/// `String::len` is the UTF-8 byte length, which over-measures any non-ASCII
/// text and breaks lines far earlier than the requested width. Layout positions
/// are column counts, so count characters instead.
pub(crate) fn text_width(data: &str) -> usize {
    data.chars().count()
}

/// A compiled layout: the output of [`Layout::compile`](crate::Layout::compile) and the input
/// to [`Doc::render`]. Callers never construct or inspect a `Doc`. `Clone`
/// and `Drop` are derived and structurally deep-safe (they touch only flat
/// `Vec`s), so no amount of document nesting can overflow the stack.
///
/// `Debug` prints the document in the DSL of [`dsl`](crate::dsl): the lines
/// joined by `@`, each run one literal. The document is a normal form, so
/// that DSL compiles to the same document.
#[derive(Clone)]
#[must_use = "a document does nothing until it is rendered"]
pub struct Doc {
    /// One entry per line, in document order; `None` is an empty line. Lines
    /// are joined by newlines when rendered, so there is no trailing newline
    /// unless the document ends in an empty line.
    pub(crate) lines: Vec<Option<ObjId>>,
    pub(crate) objs: Arena<Obj>,
    /// All text, concatenated; runs hold ranges into it.
    pub(crate) text: String,
    /// Number of pack-mark slots the renderer needs (max pack index + 1).
    /// Pack indices are dense DFS counters assigned during compilation, so
    /// the renderer keys its marks by plain vector index.
    pub(crate) packs: usize,
}

impl Doc {
    /// An empty document with the object arena's capacity reserved (callers
    /// pass the size of the representation they are lowering from).
    pub(crate) fn with_capacity(objs: usize) -> Self {
        Doc {
            lines: Vec::new(),
            objs: Arena::with_capacity(objs),
            text: String::new(),
            packs: 0,
        }
    }

    /// Opens a run: the offset [`end_run`](Self::end_run) closes it at.
    pub(crate) fn start_run(&self) -> usize {
        self.text.len()
    }

    /// Appends a text to the open run, after the space of the pad that
    /// precedes it.
    pub(crate) fn push_text(&mut self, pad: Pad, data: &str) {
        if pad == Pad::Padded {
            self.text.push(' ');
        }
        self.text.push_str(data);
    }

    /// Closes the run opened at `start` and returns its object.
    pub(crate) fn end_run(&mut self, start: usize) -> ObjId {
        self.push(ObjNode::Run(Range::new(start, self.text.len())))
    }

    /// Appends an object, whose children must already be in the arena,
    /// measuring it, and returns its id.
    pub(crate) fn push(&mut self, node: ObjNode) -> ObjId {
        let (extent, next_comp) = match node {
            // A run never contains a composition boundary.
            ObjNode::Run(range) => {
                let width = text_width(range.slice(&self.text));
                (width, width)
            }
            // A mid-line group is laid out as one opaque block, so the whole
            // group stands before the next boundary.
            ObjNode::Grp(child) => (self.objs[child].extent, self.objs[child].extent),
            ObjNode::Seq(child) | ObjNode::Nest(child) => {
                (self.objs[child].extent, self.objs[child].next_comp)
            }
            ObjNode::Pack(index, child) => {
                self.packs = self.packs.max(index as usize + 1);
                (self.objs[child].extent, self.objs[child].next_comp)
            }
            ObjNode::Comp(left, right, pad) => (
                self.objs[left].extent + pad.width() + self.objs[right].extent,
                self.objs[left].next_comp,
            ),
        };
        self.objs.push(Obj {
            node,
            extent,
            next_comp,
        })
    }
}

/// A position in the document as the DSL printer walks it.
#[derive(Copy, Clone)]
enum At {
    /// The lines from `i` on, joined by `@`.
    Lines(usize),
    /// Line `i` alone: its object, or the empty text.
    Line(usize),
    Obj(ObjId),
}

impl fmt::Debug for Doc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        dsl::write_dsl(f, At::Lines(0), |at| match at {
            At::Lines(i) if i + 1 < self.lines.len() => {
                Shape::Binary(At::Line(i), Binary::Line, At::Lines(i + 1))
            }
            At::Lines(i) | At::Line(i) => match self.lines[i] {
                None => Shape::Text(""),
                Some(obj) => self.shape(obj),
            },
            At::Obj(obj) => self.shape(obj),
        })
    }
}

impl Doc {
    fn shape(&self, obj: ObjId) -> Shape<'_, At> {
        match self.objs[obj].node {
            ObjNode::Run(range) => Shape::Text(range.slice(&self.text)),
            ObjNode::Grp(child) => Shape::Unary(Unary::Grp, At::Obj(child)),
            ObjNode::Seq(child) => Shape::Unary(Unary::Seq, At::Obj(child)),
            ObjNode::Nest(child) => Shape::Unary(Unary::Nest, At::Obj(child)),
            ObjNode::Pack(_, child) => Shape::Unary(Unary::Pack, At::Obj(child)),
            ObjNode::Comp(left, right, pad) => Shape::Binary(
                At::Obj(left),
                Binary::Comp(pad, Break::Breakable),
                At::Obj(right),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::constructors::{grp, line, null, pad, text, unpad};
    use crate::dsl;

    #[test]
    fn debug_is_the_dsl_and_is_a_normal_form() {
        let layout = line(
            pad(text("a"), text("b")),
            line(null(), pad(text("x"), grp(unpad(text("c"), text("d\n"))))),
        );
        let doc = layout.compile();
        let printed = format!("{doc:?}");
        assert_eq!(printed, r#"("a" + "b") @ "" @ "x" + grp ("c" & "d\n")"#);
        assert_eq!(
            format!("{:?}", dsl::parse(&printed).expect("parses").compile()),
            printed
        );
        assert_eq!(format!("{:?}", null().compile()), r#""""#);
    }
}
