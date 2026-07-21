use bumpalo::Bump;
use std::fmt::Debug;

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

impl<'a, T: Copy + Clone + Debug> List<'a, T> {
    pub fn get_unsafe(&'a self, index: u64) -> T {
        // Iterative walk: a recursive version overflows the native stack on
        // long lists (element counts run into the tens of thousands).
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
        fn get_unsafe_matches_model(xs in prop::collection::vec(any::<i64>(), 1..64)) {
            let mem = Bump::new();
            let list = list_of(&mem, &xs);
            for (i, &x) in xs.iter().enumerate() {
                prop_assert_eq!(list.get_unsafe(i as u64), x);
            }
        }
    }
}
