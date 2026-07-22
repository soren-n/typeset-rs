use super::layout::Attr;

// First intermediate representation: Edsl. (`Broken`, the internal step the
// `broken` pass lowers Layout through on the way to `Edsl`, lives privately in
// that pass rather than here.)
#[derive(Debug)]
pub enum Edsl<'a> {
    Null,
    Text(&'a str),
    Fix(&'a Edsl<'a>),
    Grp(&'a Edsl<'a>),
    Seq(&'a Edsl<'a>),
    Nest(&'a Edsl<'a>),
    Pack(&'a Edsl<'a>),
    Line(&'a Edsl<'a>, &'a Edsl<'a>),
    Comp(&'a Edsl<'a>, &'a Edsl<'a>, Attr),
}

// Third intermediate representation: Serial.
//
// A flat slice of leaf entries in document order; each entry is a term plus how
// it glues to what follows. The slice is always non-empty and its final entry
// is always `Last`. (Formerly a `Next`/`Last`/`Past` cons-list, but the spine
// has no structural sharing — `serialize` builds it from a `Vec` — so a slice
// is the honest shape. The load-bearing persistent lists are `serialize`'s
// internal `TermList`/`CompList` scope accumulators, not this output.)
pub type Serial<'a> = &'a [SerialEntry<'a>];

#[derive(Debug, Copy, Clone)]
pub enum SerialEntry<'a> {
    /// A term followed by a composition — a hard line break
    /// (`SerialComp::Line`) or a composition separator (`SerialComp::Comp`).
    Next(&'a Term<'a>, &'a SerialComp<'a>),
    /// The document's final term, with nothing following.
    Last(&'a Term<'a>),
}

/// A layout term: a chain of `Nest`/`Pack` wrappers over a `Null`/`Text` leaf.
///
/// This shape is invariant across the `Serial`, `LinearDoc`, `FixedDoc`, and
/// `RebuildDoc` representations — the passes between them rewrite the
/// surrounding composition structure but leave terms untouched — so a single
/// type serves all four, and terms flow through those passes by borrow from
/// the serialize arena rather than being copied. (`DenullTerm` drops `Null`
/// post-denulling, so it stays distinct.)
#[derive(Debug)]
pub enum Term<'a> {
    Null,
    Text(&'a str),
    Nest(&'a Term<'a>),
    Pack(u64, &'a Term<'a>),
}

/// A grp or seq scope, identified by the index `serialize` assigns it in
/// document pre-order. Each composition point records which scopes *open* and
/// which *close* at it, relative to the previous composition on the same line;
/// `structurize` replays those deltas to rebuild the scope graph.
///
/// Carrying open/close deltas (total size O(number of scopes)) rather than each
/// composition's full enclosing scope stack (O(depth) per composition) is what
/// keeps the grp/seq passes — serialize, linearize, fixed, structurize — linear
/// on deeply nested scopes instead of O(n^2).
#[derive(Debug, Copy, Clone)]
pub enum Scope {
    Grp(u64),
    Seq(u64),
}

#[derive(Debug)]
pub enum SerialComp<'a> {
    Line,
    /// A composition: its attributes, the scopes opening here, and the scopes
    /// closing here.
    Comp(Attr, &'a [Scope], &'a [Scope]),
}

// Fourth intermediate representation: LinearDoc.
//
// The document spine is a flat slice of line objects in document order. (It was
// once a `Cons`/`Nil` cons-list, but the spine has no structural sharing — it is
// built from a `Vec` and walked linearly — so a slice is the honest shape.) The
// per-line term/comp chain (`LinearObj`) stays a cons-list.
pub type LinearDoc<'a> = &'a [&'a LinearObj<'a>];

#[derive(Debug)]
pub enum LinearObj<'a> {
    Next(&'a Term<'a>, &'a LinearComp<'a>, &'a LinearObj<'a>),
    Last(&'a Term<'a>),
}

/// A composition: its attributes, the scopes opening here, and the scopes
/// closing here.
#[derive(Debug)]
pub struct LinearComp<'a> {
    pub attr: Attr,
    pub opens: &'a [Scope],
    pub closes: &'a [Scope],
}

// Fifth intermediate representation: FixedDoc.
//
// Like `LinearDoc`, the document spine is a flat slice of line objects in
// document order (formerly an `Eod`/`Break` cons-list, but with no sharing:
// `fixed` builds it from a `Vec` and `graphify` walks it once). The per-line
// item chain (`FixedObj`) stays a cons-list.
pub type FixedDoc<'a> = &'a [&'a FixedObj<'a>];

#[derive(Debug)]
pub enum FixedObj<'a> {
    Next(&'a FixedItem<'a>, &'a FixedComp<'a>, &'a FixedObj<'a>),
    Last(&'a FixedItem<'a>),
}

#[derive(Debug)]
pub enum FixedItem<'a> {
    Fix(&'a FixedFix<'a>),
    Term(&'a Term<'a>),
}

/// A composition: its pad flag, the scopes opening here, and the scopes
/// closing here.
#[derive(Debug)]
pub struct FixedComp<'a> {
    pub pad: bool,
    pub opens: &'a [Scope],
    pub closes: &'a [Scope],
}

#[derive(Debug)]
pub enum FixedFix<'a> {
    Next(&'a Term<'a>, &'a FixedComp<'a>, &'a FixedFix<'a>),
    Last(&'a Term<'a>),
}

// Sixth intermediate representation: RebuildDoc.
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
    Term(&'a Term<'a>),
    Fix(RFixId),
    Grp(RObjId),
    Seq(RObjId),
    Comp(RObjId, RObjId, bool),
}

#[derive(Debug, Copy, Clone)]
pub enum RebuildFix<'a> {
    Term(&'a Term<'a>),
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

// Seventh intermediate representation: DenullDoc.
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

/// A denulled term: its nest/pack wrappers (outermost first) over a non-empty
/// text leaf. The chain shape of [`Term`] carries no other information, so
/// post-denulling it is stored stripped — `rescope` factors these prop lists
/// directly.
#[derive(Debug, Clone)]
pub struct DenullTerm<'a> {
    pub props: Vec<Prop>,
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
}

// The final pass, `rescope`, lowers `DenullDoc` straight into the owned heap
// `Doc` (see `types::doc`), so there is no arena IR between the two.
