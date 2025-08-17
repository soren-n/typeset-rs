//! Pass 1: Layout → Edsl (collapse broken sequences)
//!
//! This pass transforms Layout into Edsl by tracking and collapsing
//! broken sequences. It identifies compositions that will need to be
//! broken into newlines and marks them appropriately.

use crate::compiler::types::{Attr, Broken, Edsl, Layout};
use crate::util::compose;
use bumpalo::Bump;

/// Transforms Layout into Edsl by collapsing broken sequences
pub fn broken<'b, 'a: 'b>(mem: &'b Bump, layout: Box<Layout>) -> &'b Edsl<'b> {
    fn _mark<'b, 'a: 'b>(mem: &'b Bump, layout: Box<Layout>) -> &'b Broken<'b> {
        #[allow(clippy::boxed_local)]
        fn _visit<'b, 'a: 'b>(mem: &'b Bump, layout: Box<Layout>) -> (bool, &'b Broken<'b>) {
            fn _null<'a>(mem: &'a Bump) -> &'a Broken<'a> {
                mem.alloc(Broken::Null)
            }
            fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a Broken<'a> {
                mem.alloc(Broken::Text(data))
            }
            fn _fix<'a>(mem: &'a Bump, layout: &'a Broken<'a>) -> &'a Broken<'a> {
                mem.alloc(Broken::Fix(layout))
            }
            fn _grp<'a>(mem: &'a Bump, layout: &'a Broken<'a>) -> &'a Broken<'a> {
                mem.alloc(Broken::Grp(layout))
            }
            fn _seq<'a>(mem: &'a Bump, broken: bool, layout: &'a Broken<'a>) -> &'a Broken<'a> {
                mem.alloc(Broken::Seq(broken, layout))
            }
            fn _nest<'a>(mem: &'a Bump, layout: &'a Broken<'a>) -> &'a Broken<'a> {
                mem.alloc(Broken::Nest(layout))
            }
            fn _pack<'a>(mem: &'a Bump, layout: &'a Broken<'a>) -> &'a Broken<'a> {
                mem.alloc(Broken::Pack(layout))
            }
            fn _line<'a>(
                mem: &'a Bump,
                left: &'a Broken<'a>,
                right: &'a Broken<'a>,
            ) -> &'a Broken<'a> {
                mem.alloc(Broken::Line(left, right))
            }
            fn _comp<'a>(
                mem: &'a Bump,
                left: &'a Broken<'a>,
                right: &'a Broken<'a>,
                attr: Attr,
            ) -> &'a Broken<'a> {
                mem.alloc(Broken::Comp(left, right, attr))
            }
            match *layout {
                Layout::Null => (false, _null(mem)),
                Layout::Text(data) => {
                    let data1 = mem.alloc_str(data.as_str());
                    (false, _text(mem, data1))
                }
                Layout::Fix(layout1) => {
                    let (broken, layout2) = _visit(mem, layout1.clone());
                    (broken, _fix(mem, layout2))
                }
                Layout::Grp(layout1) => {
                    let (broken, layout2) = _visit(mem, layout1.clone());
                    (broken, _grp(mem, layout2))
                }
                Layout::Seq(layout1) => {
                    let (broken, layout2) = _visit(mem, layout1.clone());
                    (broken, _seq(mem, broken, layout2))
                }
                Layout::Nest(layout1) => {
                    let (broken, layout2) = _visit(mem, layout1.clone());
                    (broken, _nest(mem, layout2))
                }
                Layout::Pack(layout1) => {
                    let (broken, layout2) = _visit(mem, layout1.clone());
                    (broken, _pack(mem, layout2))
                }
                Layout::Line(left, right) => {
                    let (_l_broken, left1) = _visit(mem, left.clone());
                    let (_r_broken, right1) = _visit(mem, right.clone());
                    (true, _line(mem, left1, right1))
                }
                Layout::Comp(left, right, attr) => {
                    let (l_broken, left1) = _visit(mem, left.clone());
                    let (r_broken, right1) = _visit(mem, right.clone());
                    let broken = l_broken || r_broken;
                    (broken, _comp(mem, left1, right1, attr))
                }
            }
        }
        let (_break, layout) = _visit(mem, layout);
        layout
    }
    fn _remove<'b, 'a: 'b, R>(
        mem: &'b Bump,
        layout: &'a Broken<'a>,
        broken: bool,
        cont: &'b dyn Fn(&'b Bump, &'b Edsl<'b>) -> R,
    ) -> R {
        fn _null<'a>(mem: &'a Bump) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Null)
        }
        fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Text(data))
        }
        fn _fix<'a>(mem: &'a Bump, layout: &'a Edsl<'a>) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Fix(layout))
        }
        fn _grp<'a>(mem: &'a Bump, layout: &'a Edsl<'a>) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Grp(layout))
        }
        fn _seq<'a>(mem: &'a Bump, layout: &'a Edsl<'a>) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Seq(layout))
        }
        fn _nest<'a>(mem: &'a Bump, layout: &'a Edsl<'a>) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Nest(layout))
        }
        fn _pack<'a>(mem: &'a Bump, layout: &'a Edsl<'a>) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Pack(layout))
        }
        fn _line<'a>(mem: &'a Bump, left: &'a Edsl<'a>, right: &'a Edsl<'a>) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Line(left, right))
        }
        fn _comp<'a>(
            mem: &'a Bump,
            left: &'a Edsl<'a>,
            right: &'a Edsl<'a>,
            attr: Attr,
        ) -> &'a Edsl<'a> {
            mem.alloc(Edsl::Comp(left, right, attr))
        }
        match layout {
            Broken::Null => cont(mem, _null(mem)),
            Broken::Text(data) => cont(mem, _text(mem, data)),
            Broken::Fix(layout1) => _remove(
                mem,
                layout1,
                false,
                compose(mem, cont, mem.alloc(|mem, layout1| _fix(mem, layout1))),
            ),
            Broken::Grp(layout1) => _remove(
                mem,
                layout1,
                false,
                compose(mem, cont, mem.alloc(|mem, layout1| _grp(mem, layout1))),
            ),
            Broken::Seq(broken, layout1) => {
                if *broken {
                    _remove(mem, layout1, true, cont)
                } else {
                    _remove(
                        mem,
                        layout1,
                        false,
                        compose(mem, cont, mem.alloc(|mem, layout2| _seq(mem, layout2))),
                    )
                }
            }
            Broken::Nest(layout1) => _remove(
                mem,
                layout1,
                broken,
                compose(mem, cont, mem.alloc(|mem, layout2| _nest(mem, layout2))),
            ),
            Broken::Pack(layout1) => _remove(
                mem,
                layout1,
                broken,
                compose(mem, cont, mem.alloc(|mem, layout2| _pack(mem, layout2))),
            ),
            Broken::Line(left, right) => _remove(
                mem,
                left,
                broken,
                mem.alloc(move |mem, left1| {
                    _remove(
                        mem,
                        right,
                        broken,
                        mem.alloc(move |mem, right1| cont(mem, _line(mem, left1, right1))),
                    )
                }),
            ),
            Broken::Comp(left, right, attr) => _remove(
                mem,
                left,
                broken,
                mem.alloc(move |mem, left1| {
                    _remove(
                        mem,
                        right,
                        broken,
                        mem.alloc(move |mem, right1| {
                            if broken && !attr.fix {
                                cont(mem, _line(mem, left1, right1))
                            } else {
                                cont(mem, _comp(mem, left1, right1, *attr))
                            }
                        }),
                    )
                }),
            ),
        }
    }
    let layout1 = _mark(mem, layout);
    _remove(mem, layout1, false, mem.alloc(|_mem, result| result))
}
