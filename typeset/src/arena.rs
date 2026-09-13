//! Typed arena indices, arenas, side tables, and ranges shared by every IR.
//!
//! Every intermediate representation stores its nodes in flat `Vec`-backed
//! arenas. An [`Id<T>`] is an index into an [`Arena<T>`] that can only be used
//! with arenas (and [`IdVec`] side tables) of the same element type, so a
//! cross-arena index mix-up is a type error rather than a silent wrong read.
//! Ids are non-zero internally, so `Option<Id<T>>` is the same four bytes as
//! an id and replaces sentinel values.

use std::fmt;
use std::hash::{Hash, Hasher};
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
impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.raw.hash(state);
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

    /// The elements in id order, paired with their ids.
    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = (Id<T>, &T)> + ExactSizeIterator {
        self.items
            .iter()
            .enumerate()
            .map(|(i, item)| (Id::from_index(i), item))
    }

    /// Appends every element of `other`, passed through `map` (which
    /// typically shifts the element's ids by this arena's former length).
    pub(crate) fn append(&mut self, other: Arena<T>, map: impl FnMut(T) -> T) {
        self.items.extend(other.items.into_iter().map(map));
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Arena::new()
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

impl<T> Index<Range<T>> for Arena<T> {
    type Output = [T];
    fn index(&self, range: Range<T>) -> &[T] {
        range.slice(&self.items)
    }
}

impl<T: fmt::Debug> fmt::Debug for Arena<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// A side table with one `V` per element of an `Arena<K>`, indexed by
/// [`Id<K>`]. Built by pushing in id order (so it stays aligned with the
/// arena by construction) or pre-filled to the arena's length.
#[derive(Clone)]
pub(crate) struct IdVec<K, V> {
    items: Vec<V>,
    _marker: PhantomData<fn() -> K>,
}

impl<K, V> IdVec<K, V> {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        IdVec {
            items: Vec::with_capacity(capacity),
            _marker: PhantomData,
        }
    }

    /// A table of `len` copies of `value`.
    pub(crate) fn filled(value: V, len: usize) -> Self
    where
        V: Clone,
    {
        IdVec {
            items: vec![value; len],
            _marker: PhantomData,
        }
    }

    /// Appends the value for the next id in order.
    pub(crate) fn push(&mut self, value: V) {
        self.items.push(value);
    }

    /// Every id in order. The iterator does not borrow the table.
    pub(crate) fn ids(
        &self,
    ) -> impl DoubleEndedIterator<Item = Id<K>> + ExactSizeIterator + use<K, V> {
        (0..self.items.len()).map(Id::from_index)
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

/// A half-open `[start, end)` range into a buffer of `T` — a slice of an
/// arena, a `Vec<T>`, or (for `Range<str>`) a text buffer — stored as two
/// `u32` offsets. Ranges are how every IR refers to a sub-sequence of a shared
/// buffer without owning a `Vec` of its own.
pub(crate) struct Range<T: ?Sized> {
    pub(crate) start: u32,
    pub(crate) end: u32,
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

    pub(crate) const EMPTY: Self = Range {
        start: 0,
        end: 0,
        _marker: PhantomData,
    };

    pub(crate) fn len(&self) -> usize {
        (self.end - self.start) as usize
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

    /// The id of the `i`-th element of the range, when the range addresses an
    /// arena.
    pub(crate) fn id_at(&self, i: usize) -> Id<T> {
        debug_assert!(i < self.len());
        Id::from_index(self.start() + i)
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

/// Appends `items` to `buf` and returns the range they occupy.
pub(crate) fn append_range<T: Clone>(buf: &mut Vec<T>, items: &[T]) -> Range<T> {
    let start = buf.len();
    buf.extend_from_slice(items);
    Range::new(start, buf.len())
}
