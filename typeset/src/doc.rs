//! The public output type: a [`Doc`] stored as a flat arena.
//!
//! The object graph (the `Comp`/`Grp`/`Nest`/… nodes) is one flat arena with
//! children referenced by arena id; runs of unbreakable text are ranges into
//! one shared run buffer, and all text is concatenated in one `String` that
//! runs range into. The spine is one optional root object per line in
//! document order (`None` for an empty line), and two side tables hold each
//! object's precomputed mid-line extents for the renderer's O(1) break
//! decisions.
//!
//! Being flat, dropping, cloning, or debug-printing a `Doc` touches a few
//! `Vec`s of shallow records and one `String`, so `Clone`, `Drop`, and `Debug`
//! derive and never recurse no matter how deeply nested the document is.

use crate::arena::{Arena, Id, IdVec, Range};
use crate::layout::Pad;

pub(crate) type ObjId = Id<ObjNode>;

/// One text of a run, with the padding that precedes it within the run. The
/// first text of a run is never padded, so a run's width is the plain sum.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunText {
    pub(crate) pad: Pad,
    pub(crate) text: Range<str>,
}

/// A node in the object arena. Children are arena ids, a run is a range into
/// the shared run buffer, so a node is a shallow record and the whole arena
/// drops/clones without recursion.
#[derive(Clone, Debug)]
pub(crate) enum ObjNode {
    /// One or more texts that never break apart.
    Run(Range<RunText>),
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
pub struct Doc {
    /// One entry per line, in document order; `None` is an empty line. Lines
    /// are joined by newlines when rendered, so there is no trailing newline
    /// unless the document ends in an empty line.
    pub(crate) lines: Vec<Option<ObjId>>,
    pub(crate) objs: Arena<ObjNode>,
    /// All runs' texts, concatenated; `ObjNode::Run` holds a range into it.
    pub(crate) runs: Vec<RunText>,
    /// All text, concatenated; runs hold ranges into it.
    pub(crate) text: String,
    /// Per-object flat extent: how many columns the object advances when laid
    /// out mid-line (`head == false`). Mid-line, `Nest`/`Pack` never emit an
    /// offset, so this is the plain sum of text widths and pads — exact and
    /// state-independent. See [`DocBuilder::finish`].
    pub(crate) extents: IdVec<ObjNode, usize>,
    /// Per-object advance to the first composition boundary, mid-line: the sum
    /// along the left spine, resolving a group as one already-laid-out block
    /// (its full extent). Exact for the same reason as `extents`.
    pub(crate) next_comps: IdVec<ObjNode, usize>,
    /// Number of pack-mark slots the renderer needs (max pack index + 1).
    /// Pack indices are dense DFS counters assigned during compilation, so
    /// the renderer keys its marks by plain vector index.
    pub(crate) packs: usize,
}

/// Appends object nodes and runs while lowering into a [`Doc`]. Nodes are
/// pushed children before parents (so a parent's child ids always already
/// exist); the spine rows are collected separately and handed to
/// [`finish`](DocBuilder::finish).
pub(crate) struct DocBuilder {
    objs: Arena<ObjNode>,
    runs: Vec<RunText>,
    text: String,
}

impl DocBuilder {
    /// A builder with the object arena's capacity reserved (callers pass the
    /// size of the representation they are lowering from).
    pub(crate) fn with_capacity(objs: usize) -> Self {
        DocBuilder {
            objs: Arena::with_capacity(objs),
            runs: Vec::new(),
            text: String::new(),
        }
    }

    /// Opens a run: the offset [`end_run`](Self::end_run) closes it at.
    pub(crate) fn start_run(&self) -> usize {
        self.runs.len()
    }

    /// Appends a text, with the pad that precedes it, to the open run.
    pub(crate) fn push_text(&mut self, pad: Pad, data: &str) {
        let start = self.text.len();
        self.text.push_str(data);
        self.runs.push(RunText {
            pad,
            text: Range::new(start, self.text.len()),
        });
    }

    /// Closes the run opened at `start` and returns its object.
    pub(crate) fn end_run(&mut self, start: usize) -> ObjId {
        self.objs
            .push(ObjNode::Run(Range::new(start, self.runs.len())))
    }

    /// Append an object node and return its id.
    pub(crate) fn obj(&mut self, node: ObjNode) -> ObjId {
        self.objs.push(node)
    }

    /// Assemble the finished document from the collected spine rows, computing
    /// the mid-line extent tables the renderer's break decisions read.
    ///
    /// Mid-line (`head == false`) neither `Nest` nor `Pack` advances the
    /// position (their offsets only apply at the head of a line), so an
    /// object's extent — and its distance to the first composition boundary —
    /// is a plain sum over the arena. The arena is postorder (children precede
    /// parents), so one forward loop suffices. Sums saturate: a saturated
    /// extent is already wider than any target width, which is all the
    /// comparisons ask.
    pub(crate) fn finish(self, lines: Vec<Option<ObjId>>) -> Doc {
        let mut packs: usize = 0;
        let mut extents: IdVec<ObjNode, usize> = IdVec::with_capacity(self.objs.len());
        let mut next_comps: IdVec<ObjNode, usize> = IdVec::with_capacity(self.objs.len());
        for (_, node) in self.objs.iter() {
            if let ObjNode::Pack(index, _) = node {
                packs = packs.max(*index as usize + 1);
            }
            let (extent, next_comp) = match node {
                // A run never contains a composition boundary.
                ObjNode::Run(range) => {
                    let width = range.slice(&self.runs).iter().fold(0usize, |acc, run| {
                        acc.saturating_add(run.pad.width())
                            .saturating_add(text_width(run.text.slice(&self.text)))
                    });
                    (width, width)
                }
                // A mid-line group is laid out as one opaque block, so the
                // whole group stands before the next boundary.
                ObjNode::Grp(child) => {
                    let extent = extents[*child];
                    (extent, extent)
                }
                ObjNode::Seq(child) | ObjNode::Nest(child) | ObjNode::Pack(_, child) => {
                    (extents[*child], next_comps[*child])
                }
                ObjNode::Comp(left, right, pad) => (
                    extents[*left]
                        .saturating_add(pad.width())
                        .saturating_add(extents[*right]),
                    next_comps[*left],
                ),
            };
            extents.push(extent);
            next_comps.push(next_comp);
        }

        Doc {
            lines,
            objs: self.objs,
            runs: self.runs,
            text: self.text,
            extents,
            next_comps,
            packs,
        }
    }
}
