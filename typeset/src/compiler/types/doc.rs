//! Final document representation - output of the compiler.
//!
//! `Doc` is a flat arena, not a `Box`-recursive tree. The document object graph
//! (the `Comp`/`Grp`/`Nest`/… nodes) is stored in two flat `Vec`s — one for
//! objects, one for fixed objects — and children are referenced by arena index
//! rather than by owning box. The spine is a `Vec<Row>` in document order.
//!
//! The point of the flat representation is that deep-safety is *structural*
//! rather than hand-maintained: dropping, cloning, or debug-printing a `Doc`
//! touches three `Vec`s of shallow records, which never recurses no matter how
//! deeply nested the document is — so `Clone`, `Drop`, and `Debug` are all
//! derived.

/// Index into a [`Doc`]'s object arena ([`Doc::objs`]).
pub(crate) type ObjId = u32;

/// Index into a [`Doc`]'s fixed-object arena ([`Doc::fixes`]).
pub(crate) type FixId = u32;

/// A node in the object arena. Children are arena indices, not owning boxes, so
/// a node is a shallow record and the whole arena drops/clones without recursion.
#[derive(Clone, Debug)]
pub(crate) enum ObjNode {
    Text(String),
    Fix(FixId),
    Grp(ObjId),
    Seq(ObjId),
    Nest(ObjId),
    Pack(u64, ObjId),
    Comp(ObjId, ObjId, bool),
}

/// A node in the fixed-object arena (the subset of objects that never break).
#[derive(Clone, Debug)]
pub(crate) enum FixNode {
    Text(String),
    Comp(FixId, FixId, bool),
}

/// Width of a literal in columns.
///
/// `String::len` is the UTF-8 byte length, which over-measures any non-ASCII
/// text and breaks lines far earlier than the requested width. Layout positions
/// are column counts, so count characters instead.
pub(crate) fn text_width(data: &str) -> usize {
    data.chars().count()
}

/// One row of the document spine, in document order.
///
/// A `Line` row is always the last row (nothing follows a line); a document that
/// ends in `Eod` simply has no `Line` row. The spine is walked front-to-back by
/// the renderer, stopping at a `Line` or running off the end (`Eod`).
#[derive(Clone, Debug)]
pub(crate) enum Row {
    Empty,
    Break(ObjId),
    Line(ObjId),
}

/// Final document representation - output of the compiler.
///
/// A flat arena: the spine is a `Vec<Row>` and the object graph lives in two
/// index-linked `Vec`s. Callers never construct or inspect a `Doc`; they pass it
/// to [`render`](crate::render()). `Clone`, `Drop`, and `Debug` are derived and
/// structurally deep-safe (they touch only flat `Vec`s), so no amount of
/// document nesting can overflow the stack.
#[derive(Clone, Debug)]
pub struct Doc {
    rows: Vec<Row>,
    objs: Vec<ObjNode>,
    fixes: Vec<FixNode>,
    /// Per-object flat extent: how many columns the object advances when laid
    /// out mid-line (`head == false`). Mid-line, `Nest`/`Pack` never emit an
    /// offset, so this is the plain sum of text widths and pads — exact and
    /// state-independent. See [`DocBuilder::finish`].
    extents: Vec<usize>,
    /// Per-object advance to the first composition boundary, mid-line: the sum
    /// along the left spine, resolving a group as one already-laid-out block
    /// (its full extent). Exact for the same reason as `extents`.
    next_comps: Vec<usize>,
}

impl Doc {
    /// The spine rows, in document order.
    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The object arena.
    pub(crate) fn objs(&self) -> &[ObjNode] {
        &self.objs
    }

    /// The fixed-object arena.
    pub(crate) fn fixes(&self) -> &[FixNode] {
        &self.fixes
    }

    /// Per-object flat (mid-line) extents.
    pub(crate) fn extents(&self) -> &[usize] {
        &self.extents
    }

    /// Per-object mid-line advances to the first composition boundary.
    pub(crate) fn next_comps(&self) -> &[usize] {
        &self.next_comps
    }
}

/// Appends object arena nodes and returns their indices while lowering into a
/// [`Doc`].
///
/// The final compiler pass ([`rescope`](crate::compiler::passes::rescope)) drives
/// this: it pushes each object/fixed-object node as it is built (children before
/// parents, so a parent's child indices always already exist) and collects the
/// spine rows separately, then calls [`finish`](DocBuilder::finish).
pub(crate) struct DocBuilder {
    objs: Vec<ObjNode>,
    fixes: Vec<FixNode>,
}

impl DocBuilder {
    pub(crate) fn new() -> Self {
        DocBuilder {
            objs: Vec::new(),
            fixes: Vec::new(),
        }
    }

    /// Append an object node and return its arena index.
    pub(crate) fn obj(&mut self, node: ObjNode) -> ObjId {
        super::push_node(&mut self.objs, node)
    }

    /// Append a fixed-object node and return its arena index.
    pub(crate) fn fix(&mut self, node: FixNode) -> FixId {
        super::push_node(&mut self.fixes, node)
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
    pub(crate) fn finish(self, rows: Vec<Row>) -> Doc {
        let mut fix_extents: Vec<usize> = Vec::with_capacity(self.fixes.len());
        for node in &self.fixes {
            let extent = match node {
                FixNode::Text(data) => text_width(data),
                FixNode::Comp(left, right, pad) => fix_extents[*left as usize]
                    .saturating_add(usize::from(*pad))
                    .saturating_add(fix_extents[*right as usize]),
            };
            fix_extents.push(extent);
        }

        let mut extents: Vec<usize> = Vec::with_capacity(self.objs.len());
        let mut next_comps: Vec<usize> = Vec::with_capacity(self.objs.len());
        for node in &self.objs {
            let (extent, next_comp) = match node {
                ObjNode::Text(data) => {
                    let width = text_width(data);
                    (width, width)
                }
                // A fixed object never contains a composition boundary.
                ObjNode::Fix(fix) => {
                    let extent = fix_extents[*fix as usize];
                    (extent, extent)
                }
                // A mid-line group is laid out as one opaque block, so the
                // whole group stands before the next boundary.
                ObjNode::Grp(child) => {
                    let extent = extents[*child as usize];
                    (extent, extent)
                }
                ObjNode::Seq(child) | ObjNode::Nest(child) | ObjNode::Pack(_, child) => {
                    (extents[*child as usize], next_comps[*child as usize])
                }
                ObjNode::Comp(left, right, pad) => (
                    extents[*left as usize]
                        .saturating_add(usize::from(*pad))
                        .saturating_add(extents[*right as usize]),
                    next_comps[*left as usize],
                ),
            };
            extents.push(extent);
            next_comps.push(next_comp);
        }

        Doc {
            rows,
            objs: self.objs,
            fixes: self.fixes,
            extents,
            next_comps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Depth chosen far past where a recursive drop/print aborted (~400-2000).
    const DEEP: usize = 50_000;

    /// Builds a single-line document whose object is `Nest^depth(Text("x"))`.
    fn deep_nest_line(depth: usize) -> Doc {
        let mut b = DocBuilder::new();
        let mut id = b.obj(ObjNode::Text("x".to_string()));
        for _ in 0..depth {
            id = b.obj(ObjNode::Nest(id));
        }
        b.finish(vec![Row::Line(id)])
    }

    #[test]
    fn deep_doc_drops_without_overflow() {
        // Structural now: dropping three `Vec`s never recurses. Kept as a guard.
        let doc = deep_nest_line(DEEP);
        drop(doc);
    }

    #[test]
    fn deep_doc_clones_and_debugs_without_overflow() {
        // Clone and the derived Debug both walk flat Vecs, never the object
        // graph, so depth cannot overflow either.
        let doc = deep_nest_line(DEEP);
        let cloned = doc.clone();
        assert_eq!(format!("{:?}", cloned), format!("{:?}", doc));
    }
}
