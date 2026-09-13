//! Final document representation - output of the compiler.
//!
//! `Doc` is a flat arena, not a `Box`-recursive tree. The document object graph
//! (the `Comp`/`Grp`/`Nest`/… nodes) is stored in two flat arenas — one for
//! objects, one for fixed objects — with children referenced by arena id
//! rather than by owning box, and all text concatenated in one shared `String`
//! that text nodes range into. The spine is one optional root object per
//! line in document order (`None` for an empty line), and two side tables
//! hold each object's precomputed mid-line extents for the renderer's O(1)
//! break decisions.
//!
//! The point of the flat representation is that deep-safety is *structural*
//! rather than hand-maintained: dropping, cloning, or debug-printing a `Doc`
//! touches a few flat `Vec`s of shallow records and one `String`, which never
//! recurses no matter how deeply nested the document is — so `Clone`, `Drop`,
//! and `Debug` are all derived.

use crate::arena::{Arena, Id, IdVec, Range};
use crate::layout::Pad;

pub(crate) type ObjId = Id<ObjNode>;
pub(crate) type FixId = Id<FixNode>;

/// A node in the object arena. Children are arena ids, not owning boxes, and
/// text is a range into the shared buffer, so a node is a shallow record and
/// the whole arena drops/clones without recursion.
#[derive(Clone, Debug)]
pub(crate) enum ObjNode {
    Text(Range<str>),
    Fix(FixId),
    Grp(ObjId),
    Seq(ObjId),
    Nest(ObjId),
    Pack(u32, ObjId),
    Comp(ObjId, ObjId, Pad),
}

/// A node in the fixed-object arena (the subset of objects that never break).
#[derive(Clone, Debug)]
pub(crate) enum FixNode {
    Text(Range<str>),
    Comp(FixId, FixId, Pad),
}

/// Width of a literal in columns.
///
/// `String::len` is the UTF-8 byte length, which over-measures any non-ASCII
/// text and breaks lines far earlier than the requested width. Layout positions
/// are column counts, so count characters instead.
pub(crate) fn text_width(data: &str) -> usize {
    data.chars().count()
}

/// Final document representation - output of the compiler.
///
/// A flat arena: the spine is one optional root object per line and the
/// object graph lives in two id-linked arenas. Callers never construct or inspect a `Doc`; they pass it
/// to [`render`](crate::render()). `Clone`, `Drop`, and `Debug` are derived and
/// structurally deep-safe (they touch only flat `Vec`s), so no amount of
/// document nesting can overflow the stack.
#[derive(Clone, Debug)]
pub struct Doc {
    /// One entry per line, in document order; `None` is an empty line. Lines
    /// are joined by newlines when rendered, so there is no trailing newline
    /// unless the document ends in an empty line.
    pub(crate) lines: Vec<Option<ObjId>>,
    pub(crate) objs: Arena<ObjNode>,
    pub(crate) fixes: Arena<FixNode>,
    /// All node text, concatenated; nodes hold ranges into it.
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

/// Appends object arena nodes and returns their ids while lowering into a
/// [`Doc`].
///
/// The final compiler pass ([`rescope`](fn@crate::rescope::rescope)) drives
/// this: it pushes each object/fixed-object node as it is built (children before
/// parents, so a parent's child ids always already exist) and collects the
/// spine rows separately, then calls [`finish`](DocBuilder::finish).
pub(crate) struct DocBuilder {
    objs: Arena<ObjNode>,
    fixes: Arena<FixNode>,
    text: String,
}

impl DocBuilder {
    /// A builder with arena capacities reserved (callers pass the size of the
    /// representation they are lowering from, a floor on the output size).
    pub(crate) fn with_capacity(objs: usize, fixes: usize) -> Self {
        DocBuilder {
            objs: Arena::with_capacity(objs),
            fixes: Arena::with_capacity(fixes),
            text: String::new(),
        }
    }

    /// Append `data` to the shared text buffer and return its range.
    pub(crate) fn text(&mut self, data: &str) -> Range<str> {
        let start = self.text.len();
        self.text.push_str(data);
        Range::new(start, self.text.len())
    }

    /// Append an object node and return its id.
    pub(crate) fn obj(&mut self, node: ObjNode) -> ObjId {
        self.objs.push(node)
    }

    /// Append a fixed-object node and return its id.
    pub(crate) fn fix(&mut self, node: FixNode) -> FixId {
        self.fixes.push(node)
    }

    /// Assemble the finished document from the collected spine rows, computing
    /// the mid-line extent tables the renderer's break decisions read.
    ///
    /// Mid-line (`head == false`) neither `Nest` nor `Pack` advances the
    /// position (their offsets only apply at the head of a line), so an
    /// object's extent — and its distance to the first composition boundary —
    /// is a plain sum over the arena. Both arenas are postorder (children
    /// precede parents), so one forward loop each suffices. Sums saturate: a
    /// saturated extent is already wider than any target width, which is all
    /// the comparisons ask.
    pub(crate) fn finish(self, lines: Vec<Option<ObjId>>) -> Doc {
        let mut packs: usize = 0;

        let mut fix_extents: IdVec<FixNode, usize> = IdVec::with_capacity(self.fixes.len());
        for (_, node) in self.fixes.iter() {
            let extent = match node {
                FixNode::Text(range) => text_width(range.slice(&self.text)),
                FixNode::Comp(left, right, pad) => fix_extents[*left]
                    .saturating_add(pad.width())
                    .saturating_add(fix_extents[*right]),
            };
            fix_extents.push(extent);
        }

        let mut extents: IdVec<ObjNode, usize> = IdVec::with_capacity(self.objs.len());
        let mut next_comps: IdVec<ObjNode, usize> = IdVec::with_capacity(self.objs.len());
        for (_, node) in self.objs.iter() {
            if let ObjNode::Pack(index, _) = node {
                packs = packs.max(*index as usize + 1);
            }
            let (extent, next_comp) = match node {
                ObjNode::Text(range) => {
                    let width = text_width(range.slice(&self.text));
                    (width, width)
                }
                // A fixed object never contains a composition boundary.
                ObjNode::Fix(fix) => {
                    let extent = fix_extents[*fix];
                    (extent, extent)
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
            fixes: self.fixes,
            text: self.text,
            extents,
            next_comps,
            packs,
        }
    }
}
