//! Typed arena indices, arenas, side tables, ranges and parent-linked trees
//! shared by every IR.
//!
//! Every intermediate representation stores its nodes in flat `Vec`-backed
//! arenas. An [`Id<T>`] is an index into an [`Arena<T>`] that can only be used
//! with arenas (and [`IdVec`] side tables) of the same element type, so a
//! cross-arena index mix-up is a type error rather than a silent wrong read.
//! Ids are non-zero internally, so `Option<Id<T>>` is the same four bytes as
//! an id and replaces sentinel values.

use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::ops::{Index, IndexMut};

/// A typed index into an [`Arena<T>`].
pub(crate) struct Id<T> {
    /// The index plus one, so that `Option<Id<T>>` has a niche.
    raw: NonZeroU32,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    /// The id of the element at `index`.
    pub(crate) fn from_index(index: usize) -> Self {
        let raw = u32::try_from(index + 1).expect("arena index exceeds u32");
        Id {
            raw: NonZeroU32::new(raw).expect("index + 1 is non-zero"),
            _marker: PhantomData,
        }
    }

    /// The element's position in its arena.
    pub(crate) fn index(self) -> usize {
        (self.raw.get() - 1) as usize
    }
}

impl<T> Copy for Id<T> {}
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}
impl<T> Eq for Id<T> {}
impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.raw.cmp(&other.raw)
    }
}
impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "#{}", self.index())
    }
}

/// A flat, append-only arena indexed by [`Id<T>`].
#[derive(Clone)]
pub(crate) struct Arena<T> {
    items: Vec<T>,
}

impl<T> Arena<T> {
    pub(crate) fn new() -> Self {
        Arena { items: Vec::new() }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Arena {
            items: Vec::with_capacity(capacity),
        }
    }

    /// Appends `item` and returns its id.
    pub(crate) fn push(&mut self, item: T) -> Id<T> {
        let id = Id::from_index(self.items.len());
        self.items.push(item);
        id
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn clear(&mut self) {
        self.items.clear();
    }

    /// Every id in order. The iterator does not borrow the arena.
    pub(crate) fn ids(&self) -> impl ExactSizeIterator<Item = Id<T>> + use<T> {
        (0..self.items.len()).map(Id::from_index)
    }

    /// Appends every element of `other`, passed through `map` (which
    /// typically shifts the element's ids by this arena's former length).
    pub(crate) fn append(&mut self, other: Arena<T>, map: impl FnMut(T) -> T) {
        self.items.extend(other.items.into_iter().map(map));
    }
}

impl<T> Index<Id<T>> for Arena<T> {
    type Output = T;
    fn index(&self, id: Id<T>) -> &T {
        &self.items[id.index()]
    }
}

impl<T> IndexMut<Id<T>> for Arena<T> {
    fn index_mut(&mut self, id: Id<T>) -> &mut T {
        &mut self.items[id.index()]
    }
}

impl<T: fmt::Debug> fmt::Debug for Arena<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(&self.items).finish()
    }
}

/// A side table with one `V` per element of an `Arena<K>`, indexed by
/// [`Id<K>`]. Built by pushing in id order (so it stays aligned with the
/// arena by construction) or reset to the arena's length.
#[derive(Clone)]
pub(crate) struct IdVec<K, V> {
    items: Vec<V>,
    _marker: PhantomData<fn() -> K>,
}

impl<K, V> IdVec<K, V> {
    pub(crate) fn new() -> Self {
        IdVec {
            items: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Appends the value for the next id in order.
    pub(crate) fn push(&mut self, value: V) {
        self.items.push(value);
    }
}

impl<K, V> Index<Id<K>> for IdVec<K, V> {
    type Output = V;
    fn index(&self, id: Id<K>) -> &V {
        &self.items[id.index()]
    }
}

impl<K, V> IndexMut<Id<K>> for IdVec<K, V> {
    fn index_mut(&mut self, id: Id<K>) -> &mut V {
        &mut self.items[id.index()]
    }
}

impl<K, V: fmt::Debug> fmt::Debug for IdVec<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.items.iter()).finish()
    }
}

/// A half-open `[start, end)` range into a buffer of `T` — a `Vec<T>` or,
/// for `Range<str>`, a text buffer — stored as two `u32` offsets. Ranges are
/// how every IR refers to a sub-sequence of a shared buffer without owning
/// a `Vec` of its own.
pub(crate) struct Range<T: ?Sized> {
    start: u32,
    end: u32,
    _marker: PhantomData<fn(&T)>,
}

impl<T: ?Sized> Range<T> {
    pub(crate) const fn new(start: usize, end: usize) -> Self {
        assert!(start <= end, "range start past end");
        assert!(end <= u32::MAX as usize, "range end exceeds u32");
        Range {
            start: start as u32,
            end: end as u32,
            _marker: PhantomData,
        }
    }

    pub(crate) fn start(&self) -> usize {
        self.start as usize
    }

    pub(crate) fn end(&self) -> usize {
        self.end as usize
    }
}

impl<T> Range<T> {
    /// The elements this range selects from `buf`.
    pub(crate) fn slice<'a>(&self, buf: &'a [T]) -> &'a [T] {
        &buf[self.start()..self.end()]
    }
}

impl Range<str> {
    /// The text this range selects from `buf`.
    pub(crate) fn slice<'a>(&self, buf: &'a str) -> &'a str {
        &buf[self.start()..self.end()]
    }
}

impl<T: ?Sized> Copy for Range<T> {}
impl<T: ?Sized> Clone for Range<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: ?Sized> PartialEq for Range<T> {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.end == other.end
    }
}
impl<T: ?Sized> Eq for Range<T> {}
impl<T: ?Sized> fmt::Debug for Range<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// One node of a [`Tree`]: its value and the node it hangs under.
#[derive(Debug, Copy, Clone)]
pub(crate) struct Node<T> {
    pub(crate) value: T,
    /// The next-outer node; `None` at the root.
    pub(crate) parent: Option<Id<Node<T>>>,
    /// Chain length from the root (a root node has depth 1).
    depth: u32,
}

/// A forest of parent-linked nodes in one arena. A chain from a node to the
/// root is the sequence of wrappers on a path into the layout; two chains
/// share their outer spine by id, so their lowest common ancestor — and the
/// nodes each has beyond it — cost a walk of the difference, never of the
/// depth.
#[derive(Debug)]
pub(crate) struct Tree<T> {
    nodes: Arena<Node<T>>,
}

impl<T> Tree<T> {
    pub(crate) fn new() -> Self {
        Tree {
            nodes: Arena::new(),
        }
    }

    /// Adds a node with `value` under `parent`.
    pub(crate) fn push(&mut self, value: T, parent: Option<Id<Node<T>>>) -> Id<Node<T>> {
        let depth = self.depth(parent) + 1;
        self.nodes.push(Node {
            value,
            parent,
            depth,
        })
    }

    fn depth(&self, id: Option<Id<Node<T>>>) -> u32 {
        id.map_or(0, |id| self.nodes[id].depth)
    }

    /// The deepest node on both `a`'s and `b`'s chains; `None` when they
    /// share nothing.
    pub(crate) fn lca(
        &self,
        a: Option<Id<Node<T>>>,
        b: Option<Id<Node<T>>>,
    ) -> Option<Id<Node<T>>> {
        let up =
            |id: Option<Id<Node<T>>>| self.nodes[id.expect("a deeper chain is non-empty")].parent;
        let (mut a, mut b) = (a, b);
        let (mut da, mut db) = (self.depth(a), self.depth(b));
        while da > db {
            a = up(a);
            da -= 1;
        }
        while db > da {
            b = up(b);
            db -= 1;
        }
        while a != b {
            a = up(a);
            b = up(b);
        }
        a
    }

    /// The nodes from `from` outward, innermost first, stopping before
    /// `upto`, which must be on `from`'s chain (`None` for the whole chain).
    pub(crate) fn ancestors(
        &self,
        from: Option<Id<Node<T>>>,
        upto: Option<Id<Node<T>>>,
    ) -> impl Iterator<Item = Id<Node<T>>> + '_ {
        std::iter::successors(from, move |&id| self.nodes[id].parent)
            .take_while(move |&id| Some(id) != upto)
    }
}

impl<T> Index<Id<Node<T>>> for Tree<T> {
    type Output = Node<T>;
    fn index(&self, id: Id<Node<T>>) -> &Node<T> {
        &self.nodes[id]
    }
}
