#![allow(dead_code)] // Complete persistent list implementation

use bumpalo::Bump;
use std::fmt::Debug;

use crate::util::compose;

#[derive(Debug)]
pub enum List<'a, T: Copy + Clone + Debug> {
    Nil,
    Cons(u64, T, &'a List<'a, T>),
}

pub fn nil<'a, T: Copy + Clone + Debug>(mem: &'a Bump) -> &'a List<'a, T> {
    mem.alloc(List::Nil)
}

pub fn cons<'a, T: Copy + Clone + Debug>(
    mem: &'a Bump,
    item: T,
    items: &'a List<'a, T>,
) -> &'a List<'a, T> {
    mem.alloc(List::Cons(items.length() + 1, item, items))
}

impl<'b, 'a: 'b, T: Copy + Clone + Debug> List<'a, T> {
    pub fn fold<R>(
        &'a self,
        mem: &'b Bump,
        nil_case: R,
        cons_case: &'a dyn Fn(&'b Bump, T, R) -> R,
    ) -> R {
        fn _visit<'b, 'a: 'b, T: Copy + Clone + Debug, R>(
            mem: &'b Bump,
            items: &'a List<'a, T>,
            nil_case: R,
            cons_case: &'a dyn Fn(&'b Bump, T, R) -> R,
            cont: &'b dyn Fn(&'b Bump, R) -> R,
        ) -> R {
            match items {
                List::Nil => cont(mem, nil_case),
                List::Cons(_, item, items1) => _visit(
                    mem,
                    items1,
                    nil_case,
                    cons_case,
                    compose(
                        mem,
                        cont,
                        mem.alloc(|mem, result| cons_case(mem, *item, result)),
                    ),
                ),
            }
        }
        _visit(
            mem,
            self,
            nil_case,
            cons_case,
            mem.alloc(|_mem, result| result),
        )
    }

    pub fn map<S: Copy + Clone + Debug>(
        &'a self,
        mem: &'b Bump,
        func: &'a dyn Fn(&'b Bump, T) -> S,
    ) -> &'b List<'b, S> {
        fn _visit<'b, 'a: 'b, A: Copy + Clone + Debug, B: Copy + Clone + Debug>(
            mem: &'b Bump,
            items: &'a List<'a, A>,
            func: &'a dyn Fn(&'b Bump, A) -> B,
            cont: &'a dyn Fn(&'b Bump, &'a List<'a, B>) -> &'a List<'a, B>,
        ) -> &'b List<'b, B> {
            match items {
                List::Nil => cont(mem, nil(mem)),
                List::Cons(_, item, items1) => _visit(
                    mem,
                    items1,
                    func,
                    compose(
                        mem,
                        cont,
                        mem.alloc(|mem, result| cons(mem, func(mem, *item), result)),
                    ),
                ),
            }
        }
        _visit(mem, self, func, mem.alloc(|_mem, result| result))
    }

    pub fn get(&'a self, index: u64) -> Option<T> {
        // Iterative walk: a recursive version overflows the native stack on
        // long lists (element counts run into the tens of thousands).
        let mut cur = self;
        let mut index = index;
        loop {
            match cur {
                List::Nil => return None,
                List::Cons(_, item, items1) => {
                    if index == 0 {
                        return Some(*item);
                    }
                    cur = items1;
                    index -= 1;
                }
            }
        }
    }

    pub fn get_unsafe(&'a self, index: u64) -> T {
        let mut cur = self;
        let mut index = index;
        loop {
            match cur {
                List::Nil => unreachable!("Invariant"),
                List::Cons(_, item, items1) => {
                    if index == 0 {
                        return *item;
                    }
                    cur = items1;
                    index -= 1;
                }
            }
        }
    }

    pub fn length(&'a self) -> u64 {
        match self {
            List::Nil => 0,
            List::Cons(length, _, _) => *length,
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Build a `List` from a slice, preserving order (`xs[0]` at the head).
    fn list_of<'a>(mem: &'a Bump, xs: &[i64]) -> &'a List<'a, i64> {
        let mut acc = nil(mem);
        for &x in xs.iter().rev() {
            acc = cons(mem, x, acc);
        }
        acc
    }

    /// Flatten a `List` back into a `Vec` for comparison against the model.
    fn to_vec<'a>(list: &'a List<'a, i64>) -> Vec<i64> {
        let mut out = Vec::new();
        let mut cur = list;
        while let List::Cons(_, x, rest) = cur {
            out.push(*x);
            cur = rest;
        }
        out
    }

    proptest! {
        /// `list_of` round-trips: the model slice and the flattened list agree.
        #[test]
        fn build_preserves_order(xs in prop::collection::vec(any::<i64>(), 0..64)) {
            let mem = Bump::new();
            prop_assert_eq!(to_vec(list_of(&mem, &xs)), xs);
        }

        #[test]
        fn length_matches_model(xs in prop::collection::vec(any::<i64>(), 0..64)) {
            let mem = Bump::new();
            prop_assert_eq!(list_of(&mem, &xs).length(), xs.len() as u64);
        }

        /// The cached length on every cons cell equals the remaining tail length.
        #[test]
        fn cached_length_is_consistent(xs in prop::collection::vec(any::<i64>(), 0..64)) {
            let mem = Bump::new();
            let mut cur = list_of(&mem, &xs);
            let mut expected = xs.len() as u64;
            while let List::Cons(len, _, rest) = cur {
                prop_assert_eq!(*len, expected);
                expected -= 1;
                cur = rest;
            }
            prop_assert_eq!(expected, 0);
        }

        #[test]
        fn get_matches_model(xs in prop::collection::vec(any::<i64>(), 0..64)) {
            let mem = Bump::new();
            let list = list_of(&mem, &xs);
            // Probe past the end as well, to cover the out-of-bounds None path.
            for i in 0..xs.len() as u64 + 2 {
                prop_assert_eq!(list.get(i), xs.get(i as usize).copied());
            }
        }

        #[test]
        fn get_unsafe_matches_model(xs in prop::collection::vec(any::<i64>(), 1..64)) {
            let mem = Bump::new();
            let list = list_of(&mem, &xs);
            for (i, &x) in xs.iter().enumerate() {
                prop_assert_eq!(list.get_unsafe(i as u64), x);
            }
        }

        /// A right fold with `cons` over `nil` reconstructs the original list.
        #[test]
        fn fold_with_cons_is_identity(xs in prop::collection::vec(any::<i64>(), 0..64)) {
            let mem = Bump::new();
            let list = list_of(&mem, &xs);
            let rebuilt = list.fold(
                &mem,
                nil(&mem),
                mem.alloc(|mem, x, acc| cons(mem, x, acc)),
            );
            prop_assert_eq!(to_vec(rebuilt), xs);
        }

        /// `fold` is a right fold: it associates as `f x0 (f x1 (... (f xn init)))`.
        /// Subtraction is not associative or commutative, so this pins the order.
        #[test]
        fn fold_folds_from_the_right(xs in prop::collection::vec(-1000i64..1000, 0..48)) {
            let mem = Bump::new();
            let list = list_of(&mem, &xs);
            let got = list.fold(&mem, 0i64, mem.alloc(|_mem, x, acc| x - acc));
            let want = xs.iter().rev().fold(0i64, |acc, &x| x - acc);
            prop_assert_eq!(got, want);
        }

        #[test]
        fn map_matches_model(xs in prop::collection::vec(any::<i64>(), 0..64)) {
            let mem = Bump::new();
            let mapped = list_of(&mem, &xs).map(&mem, mem.alloc(|_mem, x: i64| x.wrapping_mul(3)));
            let want: Vec<i64> = xs.iter().map(|x| x.wrapping_mul(3)).collect();
            prop_assert_eq!(to_vec(mapped), want);
        }
    }
}
