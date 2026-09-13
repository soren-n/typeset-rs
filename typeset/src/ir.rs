//! Intermediate-representation pieces shared by more than one pass.
//!
//! Each pass owns the type it produces (`LayoutArena` in `flatten`,
//! `SerialDoc` in `serialize`, `FixedDoc` in `split_lines`, `RebuildDoc` in
//! `resolve_scopes`, `DenullDoc` in `denull`); what lives here is the
//! vocabulary those types have in common.

use crate::arena::{Id, Range};
use crate::layout::Pad;

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

/// A nest/pack wrapper on a term, outermost first once materialized. Pack
/// indices are dense DFS counters assigned in `serialize`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum Prop {
    Nest,
    Pack(u32),
}

/// A layout term: its innermost nest/pack wrapper (a path into the shared
/// path arena, `None` for no wrappers) over a `Null`/`Text` leaf.
///
/// This shape is invariant across the serial, fixed, and rebuilt
/// representations — the passes between them rewrite the surrounding
/// composition structure but leave terms untouched — so terms flow through
/// those passes by value.
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

/// A grp or seq scope, identified by the index `serialize` assigns it in
/// document pre-order. Each composition point records which scopes *open* and
/// which *close* at it, relative to the previous composition on the same line;
/// `resolve_scopes` replays those deltas to rebuild the scope graph.
///
/// Carrying open/close deltas (total size O(number of scopes)) rather than each
/// composition's full enclosing scope stack (O(depth) per composition) is what
/// keeps the grp/seq passes linear on deeply nested scopes.
#[derive(Debug, Copy, Clone)]
pub(crate) struct Scope {
    pub(crate) kind: ScopeKind,
    pub(crate) index: u32,
}

/// Which of the two breaking disciplines a scope imposes: `Grp` breaks its
/// compositions all-or-nothing, `Seq` cascades a break forward.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    Grp,
    Seq,
}

/// An object in a per-line composition tree over terms of type `T`: the
/// shape `resolve_scopes` rebuilds (over [`Term`]) and `denull` and
/// `normalize` fold (over [`DenullTerm`]). Stored in a postorder arena, so
/// children always precede parents.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Obj<T> {
    Term(T),
    Fix(Id<Fix<T>>),
    Grp(Id<Obj<T>>),
    Seq(Id<Obj<T>>),
    Comp(Id<Obj<T>>, Id<Obj<T>>, Pad),
}

/// A fixed object: a term or an unbreakable composition of fixed objects.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Fix<T> {
    Term(T),
    Comp(Id<Fix<T>>, Id<Fix<T>>, Pad),
}
