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
        match self {
            List::Nil => None,
            List::Cons(_, item, items1) => {
                if index == 0 {
                    Some(*item)
                } else {
                    items1.get(index - 1)
                }
            }
        }
    }

    pub fn get_unsafe(&'a self, index: u64) -> T {
        match self {
            List::Nil => unreachable!("Invariant"),
            List::Cons(_, item, items1) => {
                if index == 0 {
                    *item
                } else {
                    items1.get_unsafe(index - 1)
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
