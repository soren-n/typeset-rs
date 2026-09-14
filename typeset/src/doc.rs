//! The public output type: a [`Doc`] stored as a flat arena.
//!
//! The object graph (the `Comp`/`Grp`/`Nest`/… nodes) is one flat arena with
//! children referenced by arena id; a run of unbreakable text is a range into
//! the one `String` all text is concatenated in. The spine is one optional
//! root object per line in document order (`None` for an empty line), and a
//! side table holds each object's mid-line measure, computed as the object
//! is pushed, for the renderer's O(1) break decisions.
//!
//! Being flat, dropping, cloning, or debug-printing a `Doc` touches a few
//! `Vec`s of shallow records and one `String`, so `Clone`, `Drop`, and `Debug`
//! derive and never recurse no matter how deeply nested the document is.

use crate::arena::{Arena, Id, IdVec, Range};
use crate::layout::Pad;

pub(crate) type ObjId = Id<ObjNode>;

/// A node in the object arena. Children are arena ids, a run is a range into
/// the text buffer, so a node is a shallow record and the whole arena
/// drops/clones without recursion.
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

/// Width of a literal in columns.
///
/// `String::len` is the UTF-8 byte length, which over-measures any non-ASCII
/// text and breaks lines far earlier than the requested width. Layout positions
/// are column counts, so count characters instead.
pub(crate) fn text_width(data: &str) -> usize {
    data.chars().count()
}

/// A compiled layout: the output of [`Layout::compile`](crate::Layout::compile) and the input
/// to [`Doc::render`]. Callers never construct or inspect a
/// `Doc`. `Clone`, `Drop`, and `Debug` are derived and structurally deep-safe
/// (they touch only flat `Vec`s), so no amount of document nesting can
/// overflow the stack.
#[derive(Clone, Debug)]
#[must_use = "a document does nothing until it is rendered"]
pub struct Doc {
    /// One entry per line, in document order; `None` is an empty line. Lines
    /// are joined by newlines when rendered, so there is no trailing newline
    /// unless the document ends in an empty line.
    pub(crate) lines: Vec<Option<ObjId>>,
    pub(crate) objs: Arena<ObjNode>,
    /// All text, concatenated; runs hold ranges into it.
    pub(crate) text: String,
    /// Each object's mid-line measure, computed as the object is pushed.
    pub(crate) measures: IdVec<ObjNode, Measure>,
    /// Number of pack-mark slots the renderer needs (max pack index + 1).
    /// Pack indices are dense DFS counters assigned during compilation, so
    /// the renderer keys its marks by plain vector index.
    pub(crate) packs: usize,
}

/// An object's precomputed mid-line extents, which the renderer's break
/// decisions read in O(1). Mid-line (`head == false`) neither `Nest` nor
/// `Pack` advances the position — their offsets only apply at the head of a
/// line — so both are exact, state-independent sums over the children, which
/// always precede their parent in the arena.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Measure {
    /// How many columns the object advances when laid out mid-line.
    pub(crate) extent: usize,
    /// The advance to the first composition boundary: the sum along the left
    /// spine, resolving a group as one already-laid-out block (its full
    /// extent).
    pub(crate) next_comp: usize,
}

impl Doc {
    /// An empty document with the object arena's capacity reserved (callers
    /// pass the size of the representation they are lowering from).
    pub(crate) fn with_capacity(objs: usize) -> Self {
        Doc {
            lines: Vec::new(),
            objs: Arena::with_capacity(objs),
            text: String::new(),
            measures: IdVec::with_capacity(objs),
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

    /// Appends an object node, whose children must already be in the arena,
    /// with its measure, and returns its id.
    pub(crate) fn push(&mut self, node: ObjNode) -> ObjId {
        let measure = match node {
            // A run never contains a composition boundary.
            ObjNode::Run(range) => {
                let width = text_width(range.slice(&self.text));
                Measure {
                    extent: width,
                    next_comp: width,
                }
            }
            // A mid-line group is laid out as one opaque block, so the whole
            // group stands before the next boundary.
            ObjNode::Grp(child) => Measure {
                extent: self.measures[child].extent,
                next_comp: self.measures[child].extent,
            },
            ObjNode::Seq(child) | ObjNode::Nest(child) => self.measures[child],
            ObjNode::Pack(index, child) => {
                self.packs = self.packs.max(index as usize + 1);
                self.measures[child]
            }
            ObjNode::Comp(left, right, pad) => Measure {
                extent: self.measures[left].extent + pad.width() + self.measures[right].extent,
                next_comp: self.measures[left].next_comp,
            },
        };
        self.measures.push(measure);
        self.objs.push(node)
    }
}
