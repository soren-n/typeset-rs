#![allow(dead_code)] // Complete persistent map implementation

use bumpalo::Bump;
use std::fmt::Debug;

use crate::{
    avl::{self as _avl, Avl},
    list::List,
    order::Order,
};

#[derive(Debug, Copy, Clone)]
pub enum Entry<K: Copy + Clone + Debug, V: Copy + Clone + Debug> {
    Peek(K),
    Bind(K, V),
}

pub type Map<'a, K, V> = Avl<'a, Entry<K, V>>;

fn _entry_peek<K: Copy + Clone + Debug, V: Copy + Clone + Debug>(key: K) -> Entry<K, V> {
    Entry::Peek(key)
}

fn _entry_bind<K: Copy + Clone + Debug, V: Copy + Clone + Debug>(key: K, value: V) -> Entry<K, V> {
    Entry::Bind(key, value)
}

fn _entry_key<K: Copy + Clone + Debug, V: Copy + Clone + Debug>(entry: Entry<K, V>) -> K {
    match entry {
        Entry::Peek(key) => key,
        Entry::Bind(key, _) => key,
    }
}

pub fn empty<'a, K: Copy + Clone + Debug, V: Copy + Clone + Debug>(
    mem: &'a Bump,
) -> &'a Map<'a, K, V> {
    _avl::null(mem)
}

impl<'b, 'a: 'b, K: Copy + Clone + Debug, V: Copy + Clone + Debug> Map<'a, K, V> {
    pub fn size(&'a self) -> u64 {
        _avl::get_count(self)
    }

    pub fn fold<R>(
        &'a self,
        mem: &'b Bump,
        empty_case: R,
        bind_case: &'a dyn Fn(&'b Bump, K, V, R) -> R,
    ) -> R {
        let entries = _avl::to_list(mem, self);
        entries.fold(
            mem,
            empty_case,
            mem.alloc(|mem, bind: Entry<K, V>, result| match bind {
                Entry::Peek(_) => unreachable!("Invariant"),
                Entry::Bind(key, value) => bind_case(mem, key, value, result),
            }),
        )
    }

    pub fn map<U: Copy + Clone + Debug>(
        &'a self,
        mem: &'b Bump,
        func: &'a dyn Fn(&'b Bump, V) -> U,
    ) -> &'a Map<'a, K, U> {
        _avl::map(
            mem,
            self,
            mem.alloc(|mem, bind: Entry<K, V>| match bind {
                Entry::Peek(_) => unreachable!("Invariant"),
                Entry::Bind(key, value) => _entry_bind(key, func(mem, value)),
            }),
        )
    }

    pub fn contains(&'a self, mem: &'b Bump, key_order: &'a dyn Fn(K, K) -> Order, key: K) -> bool {
        _avl::is_member(
            mem.alloc(|left, right| key_order(_entry_key(left), _entry_key(right))),
            _entry_peek(key),
            self,
        )
    }

    pub fn insert(
        &'a self,
        mem: &'b Bump,
        key_order: &'a dyn Fn(K, K) -> Order,
        key: K,
        value: V,
    ) -> &'b Map<'b, K, V> {
        _avl::insert(
            mem,
            mem.alloc(|left, right| key_order(_entry_key(left), _entry_key(right))),
            _entry_bind(key, value),
            self,
        )
    }

    pub fn remove(
        &'a self,
        mem: &'b Bump,
        key_order: &'a dyn Fn(K, K) -> Order,
        key: K,
    ) -> &'b Map<'b, K, V> {
        _avl::remove(
            mem,
            mem.alloc(|left, right| key_order(_entry_key(left), _entry_key(right))),
            _entry_peek(key),
            self,
        )
    }

    pub fn lookup(&'a self, key_order: &'a dyn Fn(K, K) -> Order, key: K) -> Option<V> {
        match self {
            Avl::Null => None,
            Avl::Node(_, _, entry, left, right) => match key_order(key, _entry_key(*entry)) {
                Order::LT => left.lookup(key_order, key),
                Order::GT => right.lookup(key_order, key),
                Order::EQ => match entry {
                    Entry::Peek(_) => None,
                    Entry::Bind(_, value) => Some(*value),
                },
            },
        }
    }

    pub fn lookup_unsafe(&'a self, key_order: &'a dyn Fn(K, K) -> Order, key: K) -> V {
        match self {
            Avl::Null => unreachable!("Invariant"),
            Avl::Node(_, _, entry, left, right) => match key_order(key, _entry_key(*entry)) {
                Order::LT => left.lookup_unsafe(key_order, key),
                Order::GT => right.lookup_unsafe(key_order, key),
                Order::EQ => match entry {
                    Entry::Peek(_) => unreachable!("Invariant"),
                    Entry::Bind(_, value) => *value,
                },
            },
        }
    }

    pub fn entries(&'a self, mem: &'b Bump) -> &'b List<'b, (K, V)> {
        let entries = _avl::to_list(mem, self);
        entries.map(
            mem,
            mem.alloc(|_mem, entry: Entry<K, V>| match entry {
                Entry::Peek(_) => unreachable!("Invariant"),
                Entry::Bind(key, value) => (key, value),
            }),
        )
    }

    pub fn keys(&'a self, mem: &'b Bump) -> &'b List<'b, K> {
        let entries = _avl::to_list(mem, self);
        entries.map(
            mem,
            mem.alloc(|_mem, entry: Entry<K, V>| match entry {
                Entry::Peek(_) => unreachable!("Invariant"),
                Entry::Bind(key, _value) => key,
            }),
        )
    }

    pub fn values(&'a self, mem: &'b Bump) -> &'b List<'b, V> {
        let entries = _avl::to_list(mem, self);
        entries.map(
            mem,
            mem.alloc(|_mem, entry: Entry<K, V>| match entry {
                Entry::Peek(_) => unreachable!("Invariant"),
                Entry::Bind(_key, value) => value,
            }),
        )
    }
}

/// Build a map from a list of entries.
///
/// Precondition: `entries` must be sorted by key and free of duplicate keys.
/// It delegates to `avl::from_list`, which assumes a sorted-unique input and
/// does not sort; violating this yields a malformed tree.
pub fn from_entries<'b, 'a: 'b, K: Copy + Clone + Debug, V: Copy + Clone + Debug>(
    mem: &'b Bump,
    entries: &'a List<'a, (K, V)>,
) -> &'b Map<'b, K, V> {
    _avl::from_list(
        mem,
        entries.map(mem, mem.alloc(|_mem, (key, value)| _entry_bind(key, value))),
    )
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::list::{cons, nil};
    use crate::order::total;
    use bumpalo::Bump;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    fn key_order(left: u64, right: u64) -> Order {
        total(left, right)
    }

    /// Flatten a `List<(u64, u64)>` into a `Vec` for comparison.
    fn pairs_to_vec<'a>(list: &'a List<'a, (u64, u64)>) -> Vec<(u64, u64)> {
        let mut out = Vec::new();
        let mut cur = list;
        while let List::Cons(_, pair, rest) = cur {
            out.push(*pair);
            cur = rest;
        }
        out
    }

    fn plain_to_vec<'a, T: Copy + Clone + Debug>(list: &'a List<'a, T>) -> Vec<T> {
        let mut out = Vec::new();
        let mut cur = list;
        while let List::Cons(_, item, rest) = cur {
            out.push(*item);
            cur = rest;
        }
        out
    }

    proptest! {
        /// Inserts, including overwrites of existing keys, agree with the model.
        #[test]
        fn insert_matches_btreemap(
            entries in prop::collection::vec((0u64..40, any::<u64>()), 0..80)
        ) {
            let mem = Bump::new();
            let mut map = empty(&mem);
            let mut model = BTreeMap::new();
            for &(k, v) in &entries {
                map = map.insert(&mem, &key_order, k, v);
                model.insert(k, v);
                prop_assert_eq!(map.size(), model.len() as u64);
                prop_assert_eq!(map.lookup(&key_order, k), Some(v));
            }
            // Full contents, in key order.
            let want: Vec<(u64, u64)> = model.iter().map(|(&k, &v)| (k, v)).collect();
            prop_assert_eq!(pairs_to_vec(map.entries(&mem)), want);
        }

        /// lookup / lookup_unsafe / contains agree with the model, present or not.
        #[test]
        fn lookup_matches_btreemap(
            entries in prop::collection::vec((0u64..40, any::<u64>()), 0..80)
        ) {
            let mem = Bump::new();
            let mut map = empty(&mem);
            let mut model = BTreeMap::new();
            for &(k, v) in &entries {
                map = map.insert(&mem, &key_order, k, v);
                model.insert(k, v);
            }
            for k in 0u64..40 {
                prop_assert_eq!(map.lookup(&key_order, k), model.get(&k).copied());
                prop_assert_eq!(map.contains(&mem, &key_order, k), model.contains_key(&k));
                if let Some(&v) = model.get(&k) {
                    prop_assert_eq!(map.lookup_unsafe(&key_order, k), v);
                }
            }
        }

        /// keys / values / entries all come out in ascending key order.
        #[test]
        fn views_are_in_key_order(
            entries in prop::collection::vec((0u64..40, any::<u64>()), 0..80)
        ) {
            let mem = Bump::new();
            let mut map = empty(&mem);
            let mut model = BTreeMap::new();
            for &(k, v) in &entries {
                map = map.insert(&mem, &key_order, k, v);
                model.insert(k, v);
            }
            let want_keys: Vec<u64> = model.keys().copied().collect();
            let want_values: Vec<u64> = model.values().copied().collect();
            let want_entries: Vec<(u64, u64)> = model.iter().map(|(&k, &v)| (k, v)).collect();
            prop_assert_eq!(plain_to_vec(map.keys(&mem)), want_keys);
            prop_assert_eq!(plain_to_vec(map.values(&mem)), want_values);
            prop_assert_eq!(pairs_to_vec(map.entries(&mem)), want_entries);
        }

        /// remove deletes the key and leaves the rest of the contents intact.
        /// (Balance is not asserted; see avl::remove.)
        #[test]
        fn remove_matches_btreemap(
            entries in prop::collection::vec((0u64..40, any::<u64>()), 1..80),
            seed in any::<u64>(),
        ) {
            let mem = Bump::new();
            let mut map = empty(&mem);
            let mut model = BTreeMap::new();
            for &(k, v) in &entries {
                map = map.insert(&mem, &key_order, k, v);
                model.insert(k, v);
            }
            let mut keys: Vec<u64> = model.keys().copied().collect();
            let mut state = seed;
            while !keys.is_empty() {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let idx = (state >> 33) as usize % keys.len();
                let k = keys.swap_remove(idx);
                map = map.remove(&mem, &key_order, k);
                model.remove(&k);
                prop_assert_eq!(map.size(), model.len() as u64);
                prop_assert_eq!(map.lookup(&key_order, k), None);
                let want: Vec<(u64, u64)> = model.iter().map(|(&k, &v)| (k, v)).collect();
                prop_assert_eq!(pairs_to_vec(map.entries(&mem)), want);
            }
        }

        /// `map` transforms every value and preserves keys and order.
        #[test]
        fn map_transforms_values(
            entries in prop::collection::vec((0u64..40, 0u64..1000), 0..80)
        ) {
            let mem = Bump::new();
            let mut map = empty(&mem);
            let mut model = BTreeMap::new();
            for &(k, v) in &entries {
                map = map.insert(&mem, &key_order, k, v);
                model.insert(k, v);
            }
            let mapped = map.map(&mem, mem.alloc(|_mem, v: u64| v.wrapping_add(7)));
            let want: Vec<(u64, u64)> =
                model.iter().map(|(&k, &v)| (k, v.wrapping_add(7))).collect();
            prop_assert_eq!(pairs_to_vec(mapped.entries(&mem)), want);
        }

        /// `fold` visits every binding once, in key order.
        #[test]
        fn fold_visits_every_binding(
            entries in prop::collection::vec((0u64..40, 0u64..1000), 0..80)
        ) {
            let mem = Bump::new();
            let mut map = empty(&mem);
            let mut model = BTreeMap::new();
            for &(k, v) in &entries {
                map = map.insert(&mem, &key_order, k, v);
                model.insert(k, v);
            }
            let collected = map.fold(
                &mem,
                nil(&mem),
                mem.alloc(|mem, k, v, acc| cons(mem, (k, v), acc)),
            );
            let want: Vec<(u64, u64)> = model.iter().map(|(&k, &v)| (k, v)).collect();
            prop_assert_eq!(pairs_to_vec(collected), want);
        }

        /// `from_entries` on sorted, unique input round-trips (its precondition).
        #[test]
        fn from_entries_round_trips(
            entries in prop::collection::vec((0u64..1000, any::<u64>()), 0..80)
        ) {
            let mem = Bump::new();
            // Dedupe by key (last write wins), then sort — the precondition.
            let mut model = BTreeMap::new();
            for &(k, v) in &entries {
                model.insert(k, v);
            }
            let sorted: Vec<(u64, u64)> = model.iter().map(|(&k, &v)| (k, v)).collect();
            let mut list = nil(&mem);
            for &(k, v) in sorted.iter().rev() {
                list = cons(&mem, (k, v), list);
            }
            let map = from_entries(&mem, list);
            prop_assert_eq!(map.size(), sorted.len() as u64);
            prop_assert_eq!(pairs_to_vec(map.entries(&mem)), sorted);
        }
    }
}
