//! Pass 3: Serial → LinearDoc (lift newlines to spine)

use crate::compiler::types::{
    Attr, LinearComp, LinearDoc, LinearObj, LinearTerm, Serial, SerialComp, SerialTerm,
};
use crate::util::compose;
use bumpalo::Bump;

pub fn linearize<'b, 'a: 'b>(mem: &'b Bump, serial: &'a Serial<'a>) -> &'b LinearDoc<'b> {
    fn _nil<'a>(mem: &'a Bump) -> &'a LinearDoc<'a> {
        mem.alloc(LinearDoc::Nil)
    }

    fn _cons<'a>(
        mem: &'a Bump,
        obj: &'a LinearObj<'a>,
        doc: &'a LinearDoc<'a>,
    ) -> &'a LinearDoc<'a> {
        mem.alloc(LinearDoc::Cons(obj, doc))
    }

    fn _next<'a>(
        mem: &'a Bump,
        comp: &'a LinearTerm<'a>,
        term: &'a LinearComp<'a>,
        obj: &'a LinearObj<'a>,
    ) -> &'a LinearObj<'a> {
        mem.alloc(LinearObj::Next(comp, term, obj))
    }

    fn _last<'a>(mem: &'a Bump, term: &'a LinearTerm<'a>) -> &'a LinearObj<'a> {
        mem.alloc(LinearObj::Last(term))
    }

    fn _null<'a>(mem: &'a Bump) -> &'a LinearTerm<'a> {
        mem.alloc(LinearTerm::Null)
    }

    fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a LinearTerm<'a> {
        mem.alloc(LinearTerm::Text(data))
    }

    fn _nest<'a>(mem: &'a Bump, term: &'a LinearTerm<'a>) -> &'a LinearTerm<'a> {
        mem.alloc(LinearTerm::Nest(term))
    }

    fn _pack<'a>(mem: &'a Bump, index: u64, term: &'a LinearTerm<'a>) -> &'a LinearTerm<'a> {
        mem.alloc(LinearTerm::Pack(index, term))
    }

    fn _comp<'a>(mem: &'a Bump, attr: Attr) -> &'a LinearComp<'a> {
        mem.alloc(LinearComp::Comp(attr))
    }

    fn _grp<'a>(mem: &'a Bump, index: u64, comp: &'a LinearComp<'a>) -> &'a LinearComp<'a> {
        mem.alloc(LinearComp::Grp(index, comp))
    }

    fn _seq<'a>(mem: &'a Bump, index: u64, comp: &'a LinearComp<'a>) -> &'a LinearComp<'a> {
        mem.alloc(LinearComp::Seq(index, comp))
    }

    fn _visit_serial<'b, 'a: 'b, R>(
        mem: &'b Bump,
        serial: &'a Serial<'a>,
        line: &'b dyn Fn(&'b Bump, &'b LinearObj<'b>) -> &'b LinearObj<'b>,
        cont: &'b dyn Fn(&'b Bump, &'b LinearDoc<'b>) -> R,
    ) -> R {
        match serial {
            Serial::Next(term, SerialComp::Line, serial1) => _visit_term(
                mem,
                term,
                mem.alloc(move |mem, term1| {
                    _visit_serial(
                        mem,
                        serial1,
                        mem.alloc(|_mem, obj| obj),
                        mem.alloc(move |mem, serial2| {
                            cont(mem, _cons(mem, line(mem, _last(mem, term1)), serial2))
                        }),
                    )
                }),
            ),
            Serial::Next(term, comp, serial1) => _visit_term(
                mem,
                term,
                mem.alloc(move |mem, term1| {
                    _visit_comp(
                        mem,
                        comp,
                        mem.alloc(move |mem, comp1| {
                            _visit_serial(
                                mem,
                                serial1,
                                compose(
                                    mem,
                                    line,
                                    mem.alloc(move |mem, obj| _next(mem, term1, comp1, obj)),
                                ),
                                cont,
                            )
                        }),
                    )
                }),
            ),
            Serial::Last(term, Serial::Past) => _visit_term(
                mem,
                term,
                mem.alloc(|mem, term1| {
                    cont(mem, _cons(mem, line(mem, _last(mem, term1)), _nil(mem)))
                }),
            ),
            _ => unreachable!("Invariant"),
        }
    }

    fn _visit_term<'b, 'a: 'b, R>(
        mem: &'b Bump,
        term: &'a SerialTerm<'a>,
        cont: &'b dyn Fn(&'b Bump, &'b LinearTerm<'b>) -> R,
    ) -> R {
        match term {
            SerialTerm::Null => cont(mem, _null(mem)),
            SerialTerm::Text(data) => cont(mem, _text(mem, data)),
            SerialTerm::Nest(term1) => _visit_term(
                mem,
                term1,
                compose(mem, cont, mem.alloc(|mem, term2| _nest(mem, term2))),
            ),
            SerialTerm::Pack(index, term1) => _visit_term(
                mem,
                term1,
                compose(mem, cont, mem.alloc(|mem, term2| _pack(mem, *index, term2))),
            ),
        }
    }

    fn _visit_comp<'b, 'a: 'b, R>(
        mem: &'b Bump,
        comp: &'a SerialComp<'a>,
        cont: &'b dyn Fn(&'b Bump, &'b LinearComp<'b>) -> R,
    ) -> R {
        match comp {
            SerialComp::Line => unreachable!("Invariant"),
            SerialComp::Comp(attr) => cont(mem, _comp(mem, *attr)),
            SerialComp::Grp(index, comp1) => _visit_comp(
                mem,
                comp1,
                compose(mem, cont, mem.alloc(|mem, comp1| _grp(mem, *index, comp1))),
            ),
            SerialComp::Seq(index, comp1) => _visit_comp(
                mem,
                comp1,
                compose(mem, cont, mem.alloc(|mem, comp1| _seq(mem, *index, comp1))),
            ),
        }
    }

    _visit_serial(
        mem,
        serial,
        mem.alloc(|_mem, obj| obj),
        mem.alloc(|_mem, doc| doc),
    )
}
