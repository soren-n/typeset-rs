//! Pass 2: Edsl → Serial (serialize in order to normalize)

use crate::compiler::types::{Attr, Edsl, Serial, SerialComp, SerialTerm};
use crate::util::compose;
use bumpalo::Bump;

pub fn serialize<'b, 'a: 'b>(mem: &'b Bump, layout: &'a Edsl<'a>) -> &'b Serial<'b> {
    fn _next<'a>(
        mem: &'a Bump,
        term: &'a SerialTerm<'a>,
        comp: &'a SerialComp<'a>,
        serial: &'a Serial<'a>,
    ) -> &'a Serial<'a> {
        mem.alloc(Serial::Next(term, comp, serial))
    }

    fn _last<'a>(
        mem: &'a Bump,
        term: &'a SerialTerm<'a>,
        serial: &'a Serial<'a>,
    ) -> &'a Serial<'a> {
        mem.alloc(Serial::Last(term, serial))
    }

    fn _past<'a>(mem: &'a Bump) -> &'a Serial<'a> {
        mem.alloc(Serial::Past)
    }

    fn _null<'a>(mem: &'a Bump) -> &'a SerialTerm<'a> {
        mem.alloc(SerialTerm::Null)
    }

    fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a SerialTerm<'a> {
        mem.alloc(SerialTerm::Text(data))
    }

    fn _nest<'a>(mem: &'a Bump, term: &'a SerialTerm<'a>) -> &'a SerialTerm<'a> {
        mem.alloc(SerialTerm::Nest(term))
    }

    fn _pack<'a>(mem: &'a Bump, index: u64, term: &'a SerialTerm<'a>) -> &'a SerialTerm<'a> {
        mem.alloc(SerialTerm::Pack(index, term))
    }

    fn _comp<'a>(mem: &'a Bump, attr: Attr) -> &'a SerialComp<'a> {
        mem.alloc(SerialComp::Comp(attr))
    }

    fn _grp<'a>(mem: &'a Bump, index: u64, comp: &'a SerialComp<'a>) -> &'a SerialComp<'a> {
        mem.alloc(SerialComp::Grp(index, comp))
    }

    fn _seq<'a>(mem: &'a Bump, index: u64, comp: &'a SerialComp<'a>) -> &'a SerialComp<'a> {
        mem.alloc(SerialComp::Seq(index, comp))
    }

    fn __line<'a>(
        mem: &'a Bump,
        term: &'a SerialTerm<'a>,
        serial: &'a Serial<'a>,
    ) -> &'a Serial<'a> {
        _next(mem, term, mem.alloc(SerialComp::Line), serial)
    }

    fn __comp<'a>(
        mem: &'a Bump,
        comps: &'a dyn Fn(&'a Bump, &'a SerialComp<'a>) -> &'a SerialComp<'a>,
        attr: Attr,
        term: &'a SerialTerm<'a>,
        serial: &'a Serial<'a>,
    ) -> &'a Serial<'a> {
        _next(mem, term, comps(mem, _comp(mem, attr)), serial)
    }

    #[allow(clippy::too_many_arguments)]
    fn _visit<'b, 'a: 'b, R>(
        mem: &'b Bump,
        i: u64,
        j: u64,
        fixed: bool,
        terms: &'b dyn Fn(&'b Bump, &'b SerialTerm<'b>) -> &'b SerialTerm<'b>,
        comps: &'b dyn Fn(&'b Bump, &'b SerialComp<'b>) -> &'b SerialComp<'b>,
        glue: &'b dyn Fn(&'b Bump, &'b SerialTerm<'b>, &'b Serial<'b>) -> &'b Serial<'b>,
        result: &'b dyn Fn(&'b Bump, &'b Serial<'b>) -> R,
        layout: &'a Edsl<'a>,
    ) -> (u64, u64, &'b dyn Fn(&'b Bump, &'b Serial<'b>) -> R) {
        match layout {
            Edsl::Null => (
                i,
                j,
                compose(
                    mem,
                    result,
                    mem.alloc(|mem, serial| glue(mem, _null(mem), serial)),
                ),
            ),
            Edsl::Text(data) => (
                i,
                j,
                compose(
                    mem,
                    result,
                    mem.alloc(|mem, serial| glue(mem, terms(mem, _text(mem, data)), serial)),
                ),
            ),
            Edsl::Fix(layout1) => _visit(mem, i, j, true, terms, comps, glue, result, layout1),
            Edsl::Grp(layout1) => _visit(
                mem,
                i + 1,
                j,
                fixed,
                terms,
                compose(mem, comps, mem.alloc(move |mem, comp| _grp(mem, i, comp))),
                glue,
                result,
                layout1,
            ),
            Edsl::Seq(layout1) => _visit(
                mem,
                i + 1,
                j,
                fixed,
                terms,
                compose(mem, comps, mem.alloc(move |mem, comp| _seq(mem, i, comp))),
                glue,
                result,
                layout1,
            ),
            Edsl::Nest(layout1) => _visit(
                mem,
                i,
                j,
                fixed,
                compose(mem, terms, mem.alloc(|mem, term| _nest(mem, term))),
                comps,
                glue,
                result,
                layout1,
            ),
            Edsl::Pack(layout1) => _visit(
                mem,
                i,
                j + 1,
                fixed,
                compose(mem, terms, mem.alloc(move |mem, term| _pack(mem, j, term))),
                comps,
                glue,
                result,
                layout1,
            ),
            Edsl::Line(left, right) => {
                let (i1, j1, result1) = _visit(
                    mem,
                    i,
                    j,
                    fixed,
                    terms,
                    comps,
                    mem.alloc(|mem, term, serial| __line(mem, term, serial)),
                    result,
                    left,
                );
                _visit(mem, i1, j1, fixed, terms, comps, glue, result1, right)
            }
            Edsl::Comp(left, right, attr) => {
                let glue1 = mem.alloc(move |mem, term, serial| {
                    let attr1 = Attr {
                        pad: attr.pad,
                        fix: fixed || attr.fix,
                    };
                    __comp(mem, comps, attr1, term, serial)
                });
                let (i1, j1, result1) = _visit(mem, i, j, fixed, terms, comps, glue1, result, left);
                _visit(mem, i1, j1, fixed, terms, comps, glue, result1, right)
            }
        }
    }

    let (_i, _j, result) = _visit(
        mem,
        0,
        0,
        false,
        mem.alloc(|_mem, x| x),
        mem.alloc(|_mem, x| x),
        mem.alloc(|mem, term, serial| _last(mem, term, serial)),
        mem.alloc(|_mem, x| x),
        layout,
    );
    result(mem, _past(mem))
}
