use bumpalo::Bump;

use crate::{
  order::{Order},
  list::{self as _list, List},
  avl::{self as _avl, AVL}
};

#[derive(Debug, Copy, Clone)]
pub enum Entry<K: Copy + Clone, V: Copy + Clone> {
  Peek(K),
  Bind(K, V)
}

pub type Map<'a, K, V> = AVL<'a, Entry<K, V>>;

fn _entry_peek<'a, K: Copy + Clone, V: Copy + Clone>(
  key: K
) -> Entry<K, V> {
  Entry::Peek(key)
}

fn _entry_bind<K: Copy + Clone, V: Copy + Clone>(
  key: K,
  value: V
) -> Entry<K, V> {
  Entry::Bind(key, value)
}

fn _entry_key<'a, K: Copy + Clone, V: Copy + Clone>(
  entry: Entry<K, V>
) -> K {
  match entry {
    Entry::Peek(key) => key,
    Entry::Bind(key, _) => key
  }
}

pub fn empty<'a, K: Copy + Clone, V: Copy + Clone>(
  mem: &'a Bump
) -> &'a Map<'a, K, V> {
  _avl::null(mem)
}

impl<'b, 'a: 'b, K: Copy + Clone, V: Copy + Clone> Map<'a, K, V> {
  pub fn size(
    &'a self
  ) -> u64 {
    _avl::get_count(self)
  }

  pub fn fold<R>(
    &'a self,
    mem: &'b Bump,
    empty_case: R,
    bind_case: &'a dyn Fn(&'b Bump, K, V, R) -> R
  ) -> R {
    let map1 = _avl::to_list(mem, self);
    map1.fold(mem, empty_case, mem.alloc(|mem, bind: Entry<K, V>, result|
    match bind {
      Entry::Peek(_) => unreachable!("Invariant"),
      Entry::Bind(key, value) =>
        bind_case(mem, key.clone(), value.clone(), result)
    }))
  }

  pub fn map<U: Copy + Clone>(
    &'a self,
    mem: &'b Bump,
    func: &'a dyn Fn(&'b Bump, V) -> U
  ) -> &'a Map<'a, K, U> {
    _avl::map(mem, self, mem.alloc(|mem, bind: Entry<K, V>|
      match bind {
        Entry::Peek(_) => unreachable!("Invariant"),
        Entry::Bind(key, value) =>
          _entry_bind(key.clone(), func(mem, value))
      }))
  }

  pub fn contains(
    &'a self,
    mem: &'b Bump,
    key_order: &'a dyn Fn(K, K) -> Order,
    key: K
  ) -> bool {
    _avl::is_member(
      mem.alloc(|left, right| key_order(_entry_key(left), _entry_key(right))),
      _entry_peek(key),
      self
    )
  }

  pub fn insert(
    &'a self,
    mem: &'b Bump,
    key_order: &'a dyn Fn(K, K) -> Order,
    key: K,
    value: V
  ) -> &'b Map<'b, K, V>{
    _avl::insert(
      mem,
      mem.alloc(|left, right| key_order(_entry_key(left), _entry_key(right))),
      _entry_bind(key, value),
      self
    )
  }

  pub fn remove(
    &'a self,
    mem: &'b Bump,
    key_order: &'a dyn Fn(K, K) -> Order,
    key: K
  ) -> &'b Map<'b, K, V>{
    _avl::remove(
      mem,
      mem.alloc(|left, right| key_order(_entry_key(left), _entry_key(right))),
      _entry_peek(key),
      self
    )
  }

  pub fn lookup(
    &'a self,
    key_order: &'a dyn Fn(K, K) -> Order,
    key: K
  ) -> Option<V> {
    match self {
      AVL::Null => None,
      AVL::Node(_, _, entry, left, right) =>
        match key_order(key, _entry_key(*entry)) {
          Order::LT => left.lookup(key_order, key),
          Order::GT => right.lookup(key_order, key),
          Order::EQ =>
            match entry {
              Entry::Peek(_) => None,
              Entry::Bind(_, value) => Some(*value)
            }
        }
    }
  }

  pub fn lookup_unsafe(
    &'a self,
    key_order: &'a dyn Fn(K, K) -> Order,
    key: K
  ) -> V {
    match self {
      AVL::Null => unreachable!("Invariant"),
      AVL::Node(_, _, entry, left, right) =>
        match key_order(key, _entry_key(*entry)) {
          Order::LT => left.lookup_unsafe(key_order, key),
          Order::GT => right.lookup_unsafe(key_order, key),
          Order::EQ =>
            match entry {
              Entry::Peek(_) => unreachable!("Invariant"),
              Entry::Bind(_, value) => *value
            }
        }
    }
  }

  pub fn entries(
    &'a self,
    mem: &'b Bump
  ) -> &'b List<'b, (K, V)> {
    self.fold(
      mem,
      _list::nil(mem),
      mem.alloc(|mem, key, value, key_values|
        _list::cons(mem, (key, value), key_values))
    )
  }

  pub fn keys(
    &'a self,
    mem: &'b Bump
  ) -> &'b List<'b, K> {
    self.fold(
      mem,
      _list::nil(mem),
      mem.alloc(|mem, key, _value, result|
        _list::cons(mem, key, result))
    )
  }

  pub fn values(
    &'a self,
    mem: &'b Bump
  ) -> &'b List<'b, V> {
    self.fold(
      mem,
      _list::nil(mem),
      mem.alloc(|mem, _key, value, result|
        _list::cons(mem, value, result))
    )
  }
}

pub fn from_entries<'b, 'a: 'b, K: Copy + Clone, V: Copy + Clone>(
  mem: &'b Bump,
  entries: &'a List<'a, (K, V)>
) -> &'b Map<'b, K, V> {
  _avl::from_list(mem, entries.map(mem, mem.alloc(|_mem, (key, value)|
  _entry_bind(key, value))))
}
