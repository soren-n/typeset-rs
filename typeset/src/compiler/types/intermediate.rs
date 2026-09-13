use super::arena::{Arena, Id, Range};
use super::layout::Attr;

// The flat layout arena.
//
// `flatten` lowers the public `Box`-recursive [`Layout`](super::layout::Layout)
// tree into this postorder arena (children precede parents). All text is
// concatenated into one buffer (returned alongside the arena) and text nodes
// hold a range into it; every later representation borrows from that buffer,
// so the node arena owns no text and drops as soon as `resolve_breaks` has
// consumed it.

pub(crate) type LayId = Id<LayoutNode>;

#[derive(Debug)]
pub(crate) enum LayoutNode {
    Null,
    Text(Range<str>),
    Fix(LayId),
    Grp(LayId),
    Seq(LayId),
    Nest(LayId),
    Pack(LayId),
    Line(LayId, LayId),
    Comp(LayId, LayId, Attr),
}

#[derive(Debug)]
pub(crate) struct LayoutArena {
    /// Node arena in postorder: children precede parents.
    pub(crate) nodes: Arena<LayoutNode>,
    pub(crate) root: LayId,
}

// Edsl.
//
// Like the layout arena, but with hard line breaks resolved: compositions
// inside a broken sequence have become `Line`s and already-broken seq wrappers
// are gone. Text is borrowed from the layout text buffer.

pub(crate) type EdslId<'a> = Id<EdslNode<'a>>;

#[derive(Debug)]
pub(crate) enum EdslNode<'a> {
    Null,
    Text(&'a str),
    Fix(EdslId<'a>),
    Grp(EdslId<'a>),
    Seq(EdslId<'a>),
    Nest(EdslId<'a>),
    Pack(EdslId<'a>),
    Line(EdslId<'a>, EdslId<'a>),
    Comp(EdslId<'a>, EdslId<'a>, Attr),
}

#[derive(Debug)]
pub(crate) struct EdslDoc<'a> {
    /// Node arena in postorder: children precede parents.
    pub(crate) nodes: Arena<EdslNode<'a>>,
    pub(crate) root: EdslId<'a>,
}

// SerialDoc.
//
// A flat list of leaf entries in document order; each entry is a term plus how
// it glues to what follows. The entry list is always non-empty and its final
// entry is always `Last`. The document owns the path arena the entries' terms
// point into and the scope buffer their deltas range into, and borrows only
// the layout text buffer.
#[derive(Debug)]
pub(crate) struct SerialDoc<'a> {
    pub(crate) entries: Vec<SerialEntry<'a>>,
    /// The shared nest/pack path arena every [`Term`]'s `path` points into.
    pub(crate) paths: Arena<PathNode>,
    /// The shared scope buffer every delta ranges into.
    pub(crate) scopes: Vec<Scope>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum SerialEntry<'a> {
    /// A term followed by a composition — a hard line break
    /// (`SerialComp::Line`) or a composition separator (`SerialComp::Comp`).
    Next(Term<'a>, SerialComp),
    /// The document's final term, with nothing following.
    Last(Term<'a>),
}

pub(crate) type PathId = Id<PathNode>;

/// One nest/pack wrapper on the DFS path to a leaf. `serialize` pushes one
/// node per `Nest`/`Pack` layout node it descends through, so sibling leaves
/// under the same wrappers share their path spine: total path storage is
/// O(input tree), not O(leaves × depth).
#[derive(Debug, Copy, Clone)]
pub(crate) struct PathNode {
    pub(crate) prop: Prop,
    /// The enclosing (next-outer) wrapper, `None` at the outermost.
    pub(crate) parent: Option<PathId>,
}

/// A layout term: its innermost nest/pack wrapper (a path into the shared
/// path arena, `None` for no wrappers) over a `Null`/`Text` leaf.
///
/// This shape is invariant across the `SerialDoc`, `FixedDoc`, and
/// `RebuildDoc` representations — the passes between them rewrite the
/// surrounding composition structure but leave terms untouched — so a single
/// type serves all three, and terms flow through those passes by value.
#[derive(Debug, Copy, Clone)]
pub(crate) struct Term<'a> {
    pub(crate) path: Option<PathId>,
    pub(crate) leaf: TermLeaf<'a>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum TermLeaf<'a> {
    Null,
    Text(&'a str),
}

/// A grp or seq scope, identified by the index `serialize` assigns it in
/// document pre-order. Each composition point records which scopes *open* and
/// which *close* at it, relative to the previous composition on the same line;
/// `resolve_scopes` replays those deltas to rebuild the scope graph.
///
/// Carrying open/close deltas (total size O(number of scopes)) rather than each
/// composition's full enclosing scope stack (O(depth) per composition) is what
/// keeps the grp/seq passes linear on deeply nested scopes.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Scope {
    Grp(u64),
    Seq(u64),
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum SerialComp {
    Line,
    /// A composition: its attributes, the scopes opening here, and the scopes
    /// closing here (ranges into the document's shared scope buffer).
    Comp(Attr, Range<Scope>, Range<Scope>),
}

// FixedDoc.
//
// Lines in document order, each line its items with the non-fixed
// compositions separating them, and maximal runs of terms joined by fixed
// compositions coalesced into single fix items.
//
// `lines` is the top-level index; the four element buffers (items, item_seps,
// run terms, run_seps) are shared across all lines and fix runs, and a line or
// fix run is just a pair of ranges into them, so building the document
// appends instead of allocating per line or per run.

/// A composition: its pad flag, the scopes opening here, and the scopes
/// closing here (ranges into the serial document's shared scope buffer).
#[derive(Debug, Copy, Clone)]
pub(crate) struct FixedComp {
    pub(crate) pad: bool,
    pub(crate) opens: Range<Scope>,
    pub(crate) closes: Range<Scope>,
}

/// A maximal run of terms joined by fixed compositions, coalesced into one
/// unbreakable item. `terms` and `seps` are ranges into [`FixedDoc`]'s shared
/// `terms` and `run_seps` buffers; `seps[i]` sits between `terms[i]` and
/// `terms[i + 1]` (so `terms.len() == seps.len() + 1`).
#[derive(Debug, Copy, Clone)]
pub(crate) struct FixRun<'a> {
    pub(crate) terms: Range<Term<'a>>,
    pub(crate) seps: Range<FixedComp>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum FixedItem<'a> {
    Term(Term<'a>),
    Fix(FixRun<'a>),
}

/// One line: ranges into [`FixedDoc`]'s `items` and `item_seps` buffers.
/// `item_seps[seps.start + i]` is the non-fixed composition between the line's
/// item `i` and item `i + 1`.
#[derive(Debug, Copy, Clone)]
pub(crate) struct FixedLine<'a> {
    pub(crate) items: Range<FixedItem<'a>>,
    pub(crate) seps: Range<FixedComp>,
}

/// The whole document, flattened. `lines` holds each line's ranges; the four
/// element buffers are shared across all lines (items and their separators)
/// and all fix runs (run terms and their separators).
#[derive(Debug)]
pub(crate) struct FixedDoc<'a> {
    pub(crate) lines: Vec<FixedLine<'a>>,
    pub(crate) items: Vec<FixedItem<'a>>,
    pub(crate) item_seps: Vec<FixedComp>,
    pub(crate) terms: Vec<Term<'a>>,
    pub(crate) run_seps: Vec<FixedComp>,
}

// RebuildDoc.
//
// A flat postorder arena, like the final `Doc`: objects and fixed objects live
// in index-linked arenas where children always precede their parents, and the
// document spine is one root object per line. Consumers fold it bottom-up with
// a plain forward loop over the arena — by the time a node is visited its
// children's results are already computed — so no walk needs a frame stack.

pub(crate) type RObjId<'a> = Id<RebuildObj<'a>>;
pub(crate) type RFixId<'a> = Id<RebuildFix<'a>>;

#[derive(Debug, Copy, Clone)]
pub(crate) enum RebuildObj<'a> {
    Term(Term<'a>),
    Fix(RFixId<'a>),
    Grp(RObjId<'a>),
    Seq(RObjId<'a>),
    Comp(RObjId<'a>, RObjId<'a>, bool),
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum RebuildFix<'a> {
    Term(Term<'a>),
    Comp(RFixId<'a>, RFixId<'a>, bool),
}

#[derive(Debug)]
pub(crate) struct RebuildDoc<'a> {
    /// One root object per line, in document order.
    pub(crate) lines: Vec<RObjId<'a>>,
    /// Object arena in postorder: children precede parents.
    pub(crate) objs: Arena<RebuildObj<'a>>,
    /// Fixed-object arena in postorder: children precede parents.
    pub(crate) fixes: Arena<RebuildFix<'a>>,
}

// DenullDoc.
//
// A flat postorder arena like `RebuildDoc`: nulls are gone, so terms are a
// stripped `(props, text)` pair rather than a wrapper chain, and the spine is
// a row list with the same semantics as the final `Doc` (a `Line` row is
// always last; a document ending in `Eod` simply has no `Line` row).

pub(crate) type DObjId<'a> = Id<DenullObj<'a>>;
pub(crate) type DFixId<'a> = Id<DenullFix<'a>>;

/// A nest/pack wrapper on a term, outermost first.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum Prop {
    Nest,
    Pack(u64),
}

/// A denulled term: its nest/pack wrappers (outermost first, as a range into
/// the document's shared prop buffer) over a non-empty text leaf. Every
/// prop-list operation downstream — `rescope`'s prefix factoring splits a list
/// into a common prefix and two leftover suffixes — yields subranges, so ranges
/// into one shared buffer replace a per-term `Vec` without any copying.
#[derive(Debug, Copy, Clone)]
pub(crate) struct DenullTerm<'a> {
    pub(crate) props: Range<Prop>,
    pub(crate) text: &'a str,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum DenullObj<'a> {
    Term(DenullTerm<'a>),
    Fix(DFixId<'a>),
    Grp(DObjId<'a>),
    Seq(DObjId<'a>),
    Comp(DObjId<'a>, DObjId<'a>, bool),
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum DenullFix<'a> {
    Term(DenullTerm<'a>),
    Comp(DFixId<'a>, DFixId<'a>, bool),
}

/// One row of the denulled document spine, in document order. Same semantics
/// as the final `Doc`'s rows: `Line` is always the last row, and a document
/// with no `Line` row ends in `Eod`.
#[derive(Debug, Copy, Clone)]
pub(crate) enum DenullRow<'a> {
    Empty,
    Break(DObjId<'a>),
    Line(DObjId<'a>),
}

#[derive(Debug)]
pub(crate) struct DenullDoc<'a> {
    /// The spine rows, in document order.
    pub(crate) rows: Vec<DenullRow<'a>>,
    /// Object arena in postorder: children precede parents.
    pub(crate) objs: Arena<DenullObj<'a>>,
    /// Fixed-object arena in postorder: children precede parents.
    pub(crate) fixes: Arena<DenullFix<'a>>,
    /// The shared prop buffer every [`DenullTerm`]'s `props` range indexes.
    pub(crate) props: Vec<Prop>,
}

// The final pass, `rescope`, lowers `DenullDoc` straight into the owned heap
// `Doc` (see `types::doc`), so there is no arena IR between the two.
