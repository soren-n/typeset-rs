#![allow(dead_code)] // Complete Avl tree implementation with utility functions

use std::cmp::max;
use std::fmt::Debug;

use bumpalo::Bump;

use crate::{
    list::{cons, nil, List},
    order::{total, Order},
    util::compose,
};

#[derive(Debug)]
pub enum Avl<'a, T: Copy + Clone + Debug> {
    Null,
    Node(u64, u64, T, &'a Avl<'a, T>, &'a Avl<'a, T>),
}

pub fn null<'a, T: Copy + Clone + Debug>(mem: &'a Bump) -> &'a Avl<'a, T> {
    mem.alloc(Avl::Null)
}

pub fn node<'a, T: Copy + Clone + Debug>(
    mem: &'a Bump,
    count: u64,
    height: u64,
    data: T,
    left: &'a Avl<'a, T>,
    right: &'a Avl<'a, T>,
) -> &'a Avl<'a, T> {
    mem.alloc(Avl::Node(count, height, data, left, right))
}

pub fn fold<'b, 'a: 'b, T: Copy + Clone + Debug, R: Copy + Clone + Debug>(
    mem: &'b Bump,
    tree: &'a Avl<'a, T>,
    null_case: R,
    node_case: &'a dyn Fn(&'b Bump, u64, u64, T, R, R) -> R,
) -> R {
    match tree {
        Avl::Null => null_case,
        Avl::Node(count, height, data, left, right) => {
            let left1 = fold(mem, left, null_case, node_case);
            let right1 = fold(mem, right, null_case, node_case);
            node_case(mem, *count, *height, *data, left1, right1)
        }
    }
}

pub fn map<'b, 'a: 'b, A: Copy + Clone + Debug, B: Copy + Clone + Debug>(
    mem: &'b Bump,
    tree: &'a Avl<'a, A>,
    func: &'a dyn Fn(&'b Bump, A) -> B,
) -> &'b Avl<'b, B> {
    fold(
        mem,
        tree,
        null(mem),
        mem.alloc(|mem, count, height, data, left, right| {
            node(mem, count, height, func(mem, data), left, right)
        }),
    )
}

pub fn get_count<'a, T: Copy + Clone + Debug>(tree: &'a Avl<'a, T>) -> u64 {
    match tree {
        Avl::Null => 0,
        Avl::Node(count, _, _, _, _) => *count,
    }
}

pub fn get_height<'a, T: Copy + Clone + Debug>(tree: &'a Avl<'a, T>) -> u64 {
    match tree {
        Avl::Null => 0,
        Avl::Node(_, height, _, _, _) => *height,
    }
}

fn _local_inbalance<'a, T: Copy + Clone + Debug>(pos: Order, tree: &'a Avl<'a, T>) -> Order {
    match tree {
        Avl::Null => Order::EQ,
        Avl::Node(_, _, _, l, r) => {
            let h_l = get_height(l);
            let h_r = get_height(r);
            let h_diff = h_l as i64 - h_r as i64;
            match pos {
                Order::EQ => {
                    if h_diff > 1 {
                        Order::LT
                    } else if h_diff < -1 {
                        Order::GT
                    } else {
                        Order::EQ
                    }
                }
                Order::LT => {
                    if h_diff > 1 {
                        Order::LT
                    } else if h_diff < 0 {
                        Order::GT
                    } else {
                        Order::EQ
                    }
                }
                Order::GT => {
                    if h_diff > 0 {
                        Order::LT
                    } else if h_diff < -1 {
                        Order::GT
                    } else {
                        Order::EQ
                    }
                }
            }
        }
    }
}

fn _local_rebalance<'b, 'a: 'b, T: Copy + Clone + Debug>(
    mem: &'b Bump,
    pos: Order,
    tree: &'a Avl<'a, T>,
) -> &'b Avl<'b, T> {
    fn _rotate_left<'b, 'a: 'b, T: Copy + Clone + Debug>(
        mem: &'b Bump,
        p: &'a Avl<'a, T>,
    ) -> &'b Avl<'b, T> {
        match p {
            Avl::Null => unreachable!("Invariant"),
            Avl::Node(c_p, _, u, a, q) => {
                let c_a = get_count(a);
                let h_a = get_height(a);
                match q {
                    Avl::Null => unreachable!("Invariant"),
                    Avl::Node(_, _, v, b, c) => {
                        let c_b = get_count(b);
                        let h_b = get_height(b);
                        let c_l = c_a + c_b + 1;
                        let h_l = max(h_a, h_b) + 1;
                        let h_r = get_height(c);
                        node(
                            mem,
                            *c_p,
                            max(h_l, h_r) + 1,
                            *v,
                            node(mem, c_l, h_l, *u, a, b),
                            c,
                        )
                    }
                }
            }
        }
    }
    fn _rotate_right<'b, 'a: 'b, T: Copy + Clone + Debug>(
        mem: &'b Bump,
        q: &'a Avl<'a, T>,
    ) -> &'b Avl<'b, T> {
        match q {
            Avl::Null => unreachable!("Invariant"),
            Avl::Node(c_q, _, v, p, c) => {
                let c_c = get_count(c);
                let h_c = get_height(c);
                match p {
                    Avl::Null => unreachable!("Invariant"),
                    Avl::Node(_, _, u, a, b) => {
                        let c_b = get_count(b);
                        let h_b = get_height(b);
                        let c_r = c_b + c_c + 1;
                        let h_l = get_height(a);
                        let h_r = max(h_b, h_c) + 1;
                        node(
                            mem,
                            *c_q,
                            max(h_l, h_r) + 1,
                            *u,
                            a,
                            node(mem, c_r, h_r, *v, b, c),
                        )
                    }
                }
            }
        }
    }
    match _local_inbalance(pos, tree) {
        Order::EQ => tree,
        Order::LT => _rotate_right(mem, tree),
        Order::GT => _rotate_left(mem, tree),
    }
}

pub fn insert<'b, 'a: 'b, T: Copy + Clone + Debug>(
    mem: &'b Bump,
    order: &'a dyn Fn(T, T) -> Order,
    data: T,
    tree: &'a Avl<'a, T>,
) -> &'b Avl<'b, T> {
    fn _visit<'b, 'a: 'b, T: Copy + Clone + Debug>(
        mem: &'b Bump,
        order: &'a dyn Fn(T, T) -> Order,
        data: T,
        tree: &'a Avl<'a, T>,
        pos: Order,
        updated: &'b dyn Fn(&'b Bump, &'b Avl<'b, T>) -> &'b Avl<'b, T>,
        inserted: &'b dyn Fn(&'b Bump, &'b Avl<'b, T>) -> &'b Avl<'b, T>,
    ) -> &'b Avl<'b, T> {
        match tree {
            Avl::Null => inserted(mem, node(mem, 1, 1, data, null(mem), null(mem))),
            Avl::Node(count, height, data1, left, right) => match order(data, *data1) {
                Order::EQ => updated(mem, node(mem, *count, *height, data, left, right)),
                Order::LT => _visit(
                    mem,
                    order,
                    data,
                    left,
                    Order::LT,
                    compose(
                        mem,
                        updated,
                        mem.alloc(|mem, left1| node(mem, *count, *height, *data1, left1, right)),
                    ),
                    compose(
                        mem,
                        inserted,
                        mem.alloc(move |mem, left1| {
                            let height1 = max(get_height(left1) + 1, *height);
                            _local_rebalance(
                                mem,
                                pos,
                                node(mem, count + 1, height1, *data1, left1, right),
                            )
                        }),
                    ),
                ),
                Order::GT => _visit(
                    mem,
                    order,
                    data,
                    right,
                    Order::GT,
                    compose(
                        mem,
                        updated,
                        mem.alloc(|mem, right1| node(mem, *count, *height, *data1, left, right1)),
                    ),
                    compose(
                        mem,
                        inserted,
                        mem.alloc(move |mem, right1| {
                            let height1 = max(get_height(right) + 1, *height);
                            _local_rebalance(
                                mem,
                                pos,
                                node(mem, count + 1, height1, *data1, left, right1),
                            )
                        }),
                    ),
                ),
            },
        }
    }
    _visit(
        mem,
        order,
        data,
        tree,
        Order::EQ,
        mem.alloc(|_mem, tree1| tree1),
        mem.alloc(|mem, tree1| _local_rebalance(mem, Order::EQ, tree1)),
    )
}

pub fn remove<'b, 'a: 'b, T: Copy + Clone + Debug>(
    mem: &'b Bump,
    order: &'a dyn Fn(T, T) -> Order,
    data: T,
    tree: &'a Avl<'a, T>,
) -> &'b Avl<'b, T> {
    fn _leftmost<'a, T: Copy + Clone + Debug>(tree: &'a Avl<'a, T>) -> T {
        match tree {
            Avl::Null => unreachable!("Invariant"),
            Avl::Node(_, _, data, Avl::Null, _) => *data,
            Avl::Node(_, _, _, left, _) => _leftmost(left),
        }
    }
    fn _rightmost<'a, T: Copy + Clone + Debug>(tree: &'a Avl<'a, T>) -> T {
        match tree {
            Avl::Null => unreachable!("Invariant"),
            Avl::Node(_, _, data, _, Avl::Null) => *data,
            Avl::Node(_, _, _, _, right) => _rightmost(right),
        }
    }
    fn _visit<'b, 'a: 'b, T: Copy + Clone + Debug, R>(
        mem: &'b Bump,
        order: &'a dyn Fn(T, T) -> Order,
        data: T,
        tree: &'a Avl<'a, T>,
        pos: Order,
        cont: &'b dyn Fn(&'b Bump, &'b Avl<'b, T>) -> R,
    ) -> R {
        match tree {
            Avl::Null => cont(mem, null(mem)),
            Avl::Node(count, height, data1, left, right) => match order(data, *data1) {
                Order::EQ => match (left, right) {
                    (Avl::Null, Avl::Null) => cont(mem, null(mem)),
                    (Avl::Null, _) => {
                        let data2 = _leftmost(right);
                        _visit(
                            mem,
                            order,
                            data2,
                            right,
                            Order::GT,
                            compose(
                                mem,
                                cont,
                                mem.alloc(move |mem, right1| {
                                    let height1 = max(get_height(right1) + 1, *height);
                                    _local_rebalance(
                                        mem,
                                        pos,
                                        node(mem, count - 1, height1, data2, left, right1),
                                    )
                                }),
                            ),
                        )
                    }
                    (_, Avl::Null) => {
                        let data2 = _rightmost(left);
                        _visit(
                            mem,
                            order,
                            data2,
                            left,
                            Order::LT,
                            compose(
                                mem,
                                cont,
                                mem.alloc(move |mem, left1| {
                                    let height1 = max(get_height(left1) + 1, *height);
                                    _local_rebalance(
                                        mem,
                                        pos,
                                        node(mem, count - 1, height1, data2, left1, right),
                                    )
                                }),
                            ),
                        )
                    }
                    (_, _) => {
                        let left_count = get_count(left);
                        let right_count = get_count(right);
                        match total(&left_count, &right_count) {
                            Order::LT => {
                                let data1 = _leftmost(right);
                                _visit(
                                    mem,
                                    order,
                                    data1,
                                    right,
                                    Order::GT,
                                    compose(
                                        mem,
                                        cont,
                                        mem.alloc(move |mem, right1| {
                                            let height1 = max(get_height(right1) + 1, *height);
                                            _local_rebalance(
                                                mem,
                                                pos,
                                                node(mem, count - 1, height1, data1, left, right1),
                                            )
                                        }),
                                    ),
                                )
                            }
                            Order::GT | Order::EQ => {
                                let data1 = _rightmost(left);
                                _visit(
                                    mem,
                                    order,
                                    data1,
                                    left,
                                    Order::LT,
                                    compose(
                                        mem,
                                        cont,
                                        mem.alloc(move |mem, left1| {
                                            let height1 = max(get_height(left1) + 1, *height);
                                            _local_rebalance(
                                                mem,
                                                pos,
                                                node(mem, count - 1, height1, data1, left1, right),
                                            )
                                        }),
                                    ),
                                )
                            }
                        }
                    }
                },
                Order::LT => _visit(
                    mem,
                    order,
                    data,
                    left,
                    Order::LT,
                    compose(
                        mem,
                        cont,
                        mem.alloc(move |mem, left| {
                            let height1 = max(get_height(left) + 1, *height);
                            _local_rebalance(
                                mem,
                                pos,
                                node(mem, count - 1, height1, data, left, right),
                            )
                        }),
                    ),
                ),
                Order::GT => _visit(
                    mem,
                    order,
                    data,
                    right,
                    Order::GT,
                    compose(
                        mem,
                        cont,
                        mem.alloc(move |mem, right| {
                            let height1 = max(get_height(right) + 1, *height);
                            _local_rebalance(
                                mem,
                                pos,
                                node(mem, count - 1, height1, data, left, right),
                            )
                        }),
                    ),
                ),
            },
        }
    }
    _visit(
        mem,
        order,
        data,
        tree,
        Order::EQ,
        mem.alloc(|mem, tree1| _local_rebalance(mem, Order::EQ, tree1)),
    )
}

pub fn is_member<'a, T: Copy + Clone + Debug>(
    order: &'a dyn Fn(T, T) -> Order,
    item: T,
    tree: &'a Avl<'a, T>,
) -> bool {
    match tree {
        Avl::Null => false,
        Avl::Node(_, _, data, left, right) => match order(item, *data) {
            Order::EQ => true,
            Order::LT => is_member(order, item, left),
            Order::GT => is_member(order, item, right),
        },
    }
}

pub fn get_member<'a, T: Copy + Clone + Debug>(index: u64, tree: &'a Avl<'a, T>) -> Option<T> {
    match tree {
        Avl::Null => None,
        Avl::Node(_, _, data, left, right) => {
            if index == 0 {
                Some(*data)
            } else {
                let left_count = get_count(left);
                if left_count <= index {
                    get_member(index - left_count, right)
                } else {
                    get_member(index, left)
                }
            }
        }
    }
}

pub fn get_leftmost<'a, T: Copy + Clone + Debug>(tree: &'a Avl<'a, T>) -> Option<T> {
    match tree {
        Avl::Null => None,
        Avl::Node(_, _, data, left, _) => match left {
            Avl::Null => Some(*data),
            _ => get_leftmost(left),
        },
    }
}

pub fn get_rightmost<'a, T: Copy + Clone + Debug>(tree: &'a Avl<'a, T>) -> Option<T> {
    match tree {
        Avl::Null => None,
        Avl::Node(_, _, data, _, right) => match right {
            Avl::Null => Some(*data),
            _ => get_rightmost(right),
        },
    }
}

pub fn to_list<'b, 'a: 'b, T: Copy + Clone + Debug>(
    mem: &'b Bump,
    tree: &'a Avl<'a, T>,
) -> &'b List<'b, T> {
    fn _visit<'b, 'a: 'b, T: Copy + Clone + Debug>(
        mem: &'b Bump,
        tree: &'a Avl<'a, T>,
        result: &'a List<'a, T>,
    ) -> &'b List<'b, T> {
        match tree {
            Avl::Null => result,
            Avl::Node(_, _, data, left, right) => {
                let result1 = cons(mem, *data, result);
                let result2 = _visit(mem, left, result1);
                _visit(mem, right, result2)
            }
        }
    }
    _visit(mem, tree, nil(mem))
}

pub fn from_list<'b, 'a: 'b, T: Copy + Clone + Debug>(
    mem: &'b Bump,
    items: &'a List<'a, T>,
) -> &'b Avl<'b, T> {
    fn _build<'b, 'a: 'b, T: Copy + Clone + Debug>(
        mem: &'b Bump,
        pos: Order,
        count: u64,
        items: &'a List<'a, T>,
        cont: &'b dyn Fn(&'b Bump, &'a List<'a, T>, u64, &'b Avl<'b, T>) -> &'b Avl<'b, T>,
    ) -> &'b Avl<'b, T> {
        match count {
            0 => cont(mem, items, 0, null(mem)),
            1 => match items {
                List::Nil => unreachable!("Invariant"),
                List::Cons(_, data, items1) => {
                    cont(mem, items1, 1, node(mem, 1, 1, *data, null(mem), null(mem)))
                }
            },
            _ => {
                let n = count - 1;
                let m = n / 2;
                match (pos, n % 2) {
                    (_, 0) => _build(
                        mem,
                        Order::LT,
                        m,
                        items,
                        mem.alloc(
                            move |mem, items1: &'a List<'a, T>, l_h, left| match items1 {
                                List::Nil => unreachable!("Invariant"),
                                List::Cons(_, data, items2) => _build(
                                    mem,
                                    Order::GT,
                                    m,
                                    items2,
                                    mem.alloc(move |mem, items3, r_h, right| {
                                        let height = max(l_h, r_h) + 1;
                                        let tree = node(mem, count, height, *data, left, right);
                                        cont(mem, items3, height, tree)
                                    }),
                                ),
                            },
                        ),
                    ),
                    (Order::EQ, _) | (Order::LT, _) => {
                        let sm = m + 1;
                        _build(
                            mem,
                            Order::LT,
                            sm,
                            items,
                            mem.alloc(
                                move |mem, items1: &'a List<'a, T>, l_h, left| match items1 {
                                    List::Nil => unreachable!("Invariant"),
                                    List::Cons(_, data, items2) => _build(
                                        mem,
                                        Order::GT,
                                        m,
                                        items2,
                                        mem.alloc(move |mem, items3, r_h, right| {
                                            let height = max(l_h, r_h) + 1;
                                            let tree = node(mem, count, height, *data, left, right);
                                            cont(mem, items3, height, tree)
                                        }),
                                    ),
                                },
                            ),
                        )
                    }
                    (Order::GT, _) => {
                        let sm = m + 1;
                        _build(
                            mem,
                            Order::LT,
                            m,
                            items,
                            mem.alloc(
                                move |mem, items1: &'a List<'a, T>, l_h, left| match items1 {
                                    List::Nil => unreachable!("Invariant"),
                                    List::Cons(_, data, items2) => _build(
                                        mem,
                                        Order::GT,
                                        sm,
                                        items2,
                                        mem.alloc(move |mem, items3, r_h, right| {
                                            let height = max(l_h, r_h) + 1;
                                            let tree = node(mem, count, height, *data, left, right);
                                            cont(mem, items3, height, tree)
                                        }),
                                    ),
                                },
                            ),
                        )
                    }
                }
            }
        }
    }
    _build(
        mem,
        Order::EQ,
        items.length(),
        items,
        mem.alloc(|_, _, _, result| result),
    )
}
