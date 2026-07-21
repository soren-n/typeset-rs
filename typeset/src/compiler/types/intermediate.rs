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

// Third intermediate representation: Serial
#[derive(Debug)]
pub enum Serial<'a> {
    Next(&'a Term<'a>, &'a SerialComp<'a>, &'a Serial<'a>),
    Last(&'a Term<'a>, &'a Serial<'a>),
    Past,
}

/// A layout term: a chain of `Nest`/`Pack` wrappers over a `Null`/`Text` leaf.
///
/// This shape is invariant across the `Serial`, `LinearDoc`, `FixedDoc`, and
/// `RebuildDoc` representations — the passes between them rewrite the
/// surrounding composition structure but leave terms untouched — so a single
/// type serves all four. (`GraphTerm` additionally carries `Fix`, and
/// `DenullTerm` drops `Null` post-denulling, so those stay distinct.)
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

// Fourth intermediate representation: LinearDoc
#[derive(Debug)]
pub enum LinearDoc<'a> {
    Nil,
    Cons(&'a LinearObj<'a>, &'a LinearDoc<'a>),
}

#[derive(Debug)]
pub enum LinearObj<'a> {
    Next(&'a Term<'a>, &'a LinearComp<'a>, &'a LinearObj<'a>),
    Last(&'a Term<'a>),
}

#[derive(Debug)]
pub enum LinearComp<'a> {
    /// A composition: its attributes, the scopes opening here, and the scopes
    /// closing here.
    Comp(Attr, &'a [Scope], &'a [Scope]),
}

// Fifth intermediate representation: FixedDoc
#[derive(Debug)]
pub enum FixedDoc<'a> {
    Eod,
    Break(&'a FixedObj<'a>, &'a FixedDoc<'a>),
}

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

#[derive(Debug)]
pub enum FixedComp<'a> {
    /// A composition: its pad flag, the scopes opening here, and the scopes
    /// closing here.
    Comp(bool, &'a [Scope], &'a [Scope]),
}

#[derive(Debug)]
pub enum FixedFix<'a> {
    Next(&'a Term<'a>, &'a FixedComp<'a>, &'a FixedFix<'a>),
    Last(&'a Term<'a>),
}

// Sixth intermediate representation: RebuildDoc
#[derive(Debug)]
pub enum RebuildDoc<'a> {
    Eod,
    Break(&'a RebuildObj<'a>, &'a RebuildDoc<'a>),
}

#[derive(Debug)]
pub enum RebuildObj<'a> {
    Term(&'a Term<'a>),
    Fix(&'a RebuildFix<'a>),
    Grp(&'a RebuildObj<'a>),
    Seq(&'a RebuildObj<'a>),
    Comp(&'a RebuildObj<'a>, &'a RebuildObj<'a>, bool),
}

#[derive(Debug)]
pub enum RebuildFix<'a> {
    Term(&'a Term<'a>),
    Comp(&'a RebuildFix<'a>, &'a RebuildFix<'a>, bool),
}

// Seventh intermediate representation: DenullDoc
#[derive(Debug)]
pub enum DenullDoc<'a> {
    Eod,
    Line(&'a DenullObj<'a>),
    Empty(&'a DenullDoc<'a>),
    Break(&'a DenullObj<'a>, &'a DenullDoc<'a>),
}

#[derive(Debug)]
pub enum DenullObj<'a> {
    Term(&'a DenullTerm<'a>),
    Fix(&'a DenullFix<'a>),
    Grp(&'a DenullObj<'a>),
    Seq(&'a DenullObj<'a>),
    Comp(&'a DenullObj<'a>, &'a DenullObj<'a>, bool),
}

#[derive(Debug)]
pub enum DenullFix<'a> {
    Term(&'a DenullTerm<'a>),
    Comp(&'a DenullFix<'a>, &'a DenullFix<'a>, bool),
}

#[derive(Debug)]
pub enum DenullTerm<'a> {
    Text(&'a str),
    Nest(&'a DenullTerm<'a>),
    Pack(u64, &'a DenullTerm<'a>),
}

// The final pass, `rescope`, lowers `DenullDoc` straight into the owned heap
// `Doc` (see `types::doc`), so there is no arena IR between the two.
