use super::layout::Attr;

// The flat layout arena.
//
// `flatten` lowers the public `Box`-recursive [`Layout`](super::layout::Layout)
// tree into this postorder arena (children precede parents) as the pipeline's
// entry step — the one place that walks owning boxes. Text is moved out of the
// tree here; every later representation borrows it from this arena.

/// Index into a [`LayoutArena`]'s node list.
pub type LayId = u32;

#[derive(Debug)]
pub enum LayoutNode {
    Null,
    Text(String),
    Fix(LayId),
    Grp(LayId),
    Seq(LayId),
    Nest(LayId),
    Pack(LayId),
    Line(LayId, LayId),
    Comp(LayId, LayId, Attr),
}

#[derive(Debug)]
pub struct LayoutArena {
    /// Node arena in postorder: children precede parents.
    pub nodes: Vec<LayoutNode>,
    pub root: LayId,
}

// Edsl.
//
// Like the layout arena, but with hard line breaks resolved: compositions
// inside a broken sequence have become `Line`s and already-broken seq wrappers
// are gone. Owned; text is borrowed from the layout arena.

/// Index into an [`EdslDoc`]'s node list.
pub type EdslId = u32;

#[derive(Debug)]
pub enum EdslNode<'a> {
    Null,
    Text(&'a str),
    Fix(EdslId),
    Grp(EdslId),
    Seq(EdslId),
    Nest(EdslId),
    Pack(EdslId),
    Line(EdslId, EdslId),
    Comp(EdslId, EdslId, Attr),
}

#[derive(Debug)]
pub struct EdslDoc<'a> {
    /// Node arena in postorder: children precede parents.
    pub nodes: Vec<EdslNode<'a>>,
    pub root: EdslId,
}

// SerialDoc.
//
// A flat list of leaf entries in document order; each entry is a term plus how
// it glues to what follows. The entry list is always non-empty and its final
// entry is always `Last`. The document also owns the path arena the entries'
// terms point into and the scope buffer their deltas range into, so nothing
// in it references `serialize`'s internal bump — the output borrows only the
// layout arena's text. (The load-bearing persistent lists are `serialize`'s
// internal `CompList` scope accumulators, not this output.)
#[derive(Debug)]
pub struct SerialDoc<'a> {
    pub entries: Vec<SerialEntry<'a>>,
    /// The shared nest/pack path arena every [`Term`]'s `path` points into.
    pub paths: Vec<PathNode>,
    /// The shared scope buffer every delta's [`ScopeRange`] indexes.
    pub scopes: Vec<Scope>,
}

#[derive(Debug, Copy, Clone)]
pub enum SerialEntry<'a> {
    /// A term followed by a composition — a hard line break
    /// (`SerialComp::Line`) or a composition separator (`SerialComp::Comp`).
    Next(Term<'a>, SerialComp),
    /// The document's final term, with nothing following.
    Last(Term<'a>),
}

/// Index into a [`SerialDoc`]'s path arena.
pub type PathId = u32;

/// Sentinel for the empty path (no nest/pack wrappers).
pub const NO_PATH: PathId = u32::MAX;

/// One nest/pack wrapper on the DFS path to a leaf. `serialize` pushes one
/// node per `Nest`/`Pack` layout node it descends through, so sibling leaves
/// under the same wrappers *share* their path spine: total path storage is
/// O(input tree), not O(leaves × depth) as the old per-leaf wrapper chains
/// were.
#[derive(Debug, Copy, Clone)]
pub struct PathNode {
    pub prop: Prop,
    /// The enclosing (next-outer) wrapper, or [`NO_PATH`] at the outermost.
    pub parent: PathId,
}

/// A layout term: its innermost nest/pack wrapper (a path into the shared
/// path arena, [`NO_PATH`] for none) over a `Null`/`Text` leaf.
///
/// This shape is invariant across the `SerialDoc`, `FixedDoc`, and
/// `RebuildDoc` representations — the passes between them rewrite the
/// surrounding composition structure but leave terms untouched — so a single
/// type serves all three, and terms flow through those passes by value.
/// (`DenullTerm` drops `Null` post-denulling, so it stays distinct.)
#[derive(Debug, Copy, Clone)]
pub struct Term<'a> {
    pub path: PathId,
    pub leaf: TermLeaf<'a>,
}

#[derive(Debug, Copy, Clone)]
pub enum TermLeaf<'a> {
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
/// keeps the grp/seq passes — serialize, split_lines, resolve_scopes — linear
/// on deeply nested scopes instead of O(n^2).
#[derive(Debug, Copy, Clone)]
pub enum Scope {
    Grp(u64),
    Seq(u64),
}

/// A range of scopes in the document's shared scope buffer
/// ([`SerialDoc::scopes`]).
#[derive(Debug, Copy, Clone)]
pub struct ScopeRange {
    pub start: u32,
    pub end: u32,
}

impl ScopeRange {
    /// The scopes this range selects from the document's shared buffer.
    pub fn slice<'s>(&self, buf: &'s [Scope]) -> &'s [Scope] {
        &buf[self.start as usize..self.end as usize]
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SerialComp {
    Line,
    /// A composition: its attributes, the scopes opening here, and the scopes
    /// closing here (ranges into the document's shared scope buffer).
    Comp(Attr, ScopeRange, ScopeRange),
}

// FixedDoc.
//
// An owned flat structure: lines in document order, each line its items with
// the non-fixed compositions separating them, and maximal runs of terms joined
// by fixed compositions coalesced into single fix items. (This replaces the
// former LinearDoc + cons-list FixedDoc pair — splitting lines and coalescing
// fixed runs happen in one sweep over the serial entries.)

/// A composition: its pad flag, the scopes opening here, and the scopes
/// closing here (ranges into the serial document's shared scope buffer).
#[derive(Debug, Copy, Clone)]
pub struct FixedComp {
    pub pad: bool,
    pub opens: ScopeRange,
    pub closes: ScopeRange,
}

/// A maximal run of terms joined by fixed compositions, coalesced into one
/// unbreakable item. `seps[i]` sits between `terms[i]` and `terms[i + 1]`.
#[derive(Debug)]
pub struct FixRun<'a> {
    pub terms: Vec<Term<'a>>,
    pub seps: Vec<FixedComp>,
}

#[derive(Debug)]
pub enum FixedItem<'a> {
    Term(Term<'a>),
    Fix(FixRun<'a>),
}

/// One line: its items and the non-fixed compositions separating them.
/// `seps[i]` sits between `items[i]` and `items[i + 1]`.
#[derive(Debug)]
pub struct FixedLine<'a> {
    pub items: Vec<FixedItem<'a>>,
    pub seps: Vec<FixedComp>,
}

#[derive(Debug)]
pub struct FixedDoc<'a> {
    pub lines: Vec<FixedLine<'a>>,
}

// RebuildDoc.
//
// A flat postorder arena, like the final `Doc`: objects and fixed objects live
// in index-linked `Vec`s where children always precede their parents, and the
// document spine is one root object per line. Consumers fold it bottom-up with
// a plain forward loop over the arena — by the time a node is visited its
// children's results are already computed — so no walk needs a frame stack.

/// Index into a [`RebuildDoc`]'s object arena.
pub type RObjId = u32;

/// Index into a [`RebuildDoc`]'s fixed-object arena.
pub type RFixId = u32;

#[derive(Debug, Copy, Clone)]
pub enum RebuildObj<'a> {
    Term(Term<'a>),
    Fix(RFixId),
    Grp(RObjId),
    Seq(RObjId),
    Comp(RObjId, RObjId, bool),
}

#[derive(Debug, Copy, Clone)]
pub enum RebuildFix<'a> {
    Term(Term<'a>),
    Comp(RFixId, RFixId, bool),
}

#[derive(Debug)]
pub struct RebuildDoc<'a> {
    /// One root object per line, in document order.
    pub lines: Vec<RObjId>,
    /// Object arena in postorder: children precede parents.
    pub objs: Vec<RebuildObj<'a>>,
    /// Fixed-object arena in postorder: children precede parents.
    pub fixes: Vec<RebuildFix<'a>>,
}

// DenullDoc.
//
// A flat postorder arena like `RebuildDoc`, but owned (no bump arena backs
// it): nulls are gone, so terms are a stripped `(props, text)` pair rather
// than a wrapper chain, and the spine is a row list with the same semantics
// as the final `Doc` (a `Line` row is always last; a document ending in `Eod`
// simply has no `Line` row).

/// Index into a [`DenullDoc`]'s object arena.
pub type DObjId = u32;

/// Index into a [`DenullDoc`]'s fixed-object arena.
pub type DFixId = u32;

/// A nest/pack wrapper on a term, outermost first.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Prop {
    Nest,
    Pack(u64),
}

/// A term's prop list: a `start..end` range into the document's shared prop
/// buffer ([`DenullDoc::props`]). Every prop-list operation downstream —
/// `rescope`'s prefix factoring splits a list into a common prefix and two
/// leftover suffixes — yields subranges, so ranges into one shared buffer
/// replace a per-term `Vec` without any copying.
#[derive(Debug, Copy, Clone)]
pub struct Props {
    pub start: u32,
    pub end: u32,
}

impl Props {
    /// The props this range selects from the document's shared buffer.
    pub fn slice<'p>(&self, buf: &'p [Prop]) -> &'p [Prop] {
        &buf[self.start as usize..self.end as usize]
    }
}

/// A denulled term: its nest/pack wrappers (outermost first, as a range into
/// the document's shared prop buffer) over a non-empty text leaf. The chain
/// shape of [`Term`] carries no other information, so post-denulling it is
/// stored stripped — `rescope` factors these prop lists directly.
#[derive(Debug, Copy, Clone)]
pub struct DenullTerm<'a> {
    pub props: Props,
    pub text: &'a str,
}

#[derive(Debug, Clone)]
pub enum DenullObj<'a> {
    Term(DenullTerm<'a>),
    Fix(DFixId),
    Grp(DObjId),
    Seq(DObjId),
    Comp(DObjId, DObjId, bool),
}

#[derive(Debug, Clone)]
pub enum DenullFix<'a> {
    Term(DenullTerm<'a>),
    Comp(DFixId, DFixId, bool),
}

/// One row of the denulled document spine, in document order. Same semantics
/// as the final `Doc`'s rows: `Line` is always the last row, and a document
/// with no `Line` row ends in `Eod`.
#[derive(Debug, Copy, Clone)]
pub enum DenullRow {
    Empty,
    Break(DObjId),
    Line(DObjId),
}

#[derive(Debug)]
pub struct DenullDoc<'a> {
    /// The spine rows, in document order.
    pub rows: Vec<DenullRow>,
    /// Object arena in postorder: children precede parents.
    pub objs: Vec<DenullObj<'a>>,
    /// Fixed-object arena in postorder: children precede parents.
    pub fixes: Vec<DenullFix<'a>>,
    /// The shared prop buffer every [`DenullTerm`]'s `props` range indexes.
    pub props: Vec<Prop>,
}

// The final pass, `rescope`, lowers `DenullDoc` straight into the owned heap
// `Doc` (see `types::doc`), so there is no arena IR between the two.
