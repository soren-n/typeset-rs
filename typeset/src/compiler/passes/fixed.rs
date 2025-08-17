//! Pass 4: LinearDoc → FixedDoc (coalesce fixed comps)

use crate::compiler::types::{
    FixedComp, FixedDoc, FixedFix, FixedItem, FixedObj, FixedTerm, LinearComp, LinearDoc,
    LinearObj, LinearTerm,
};
use crate::util::compose;
use bumpalo::Bump;

pub fn fixed<'b, 'a: 'b>(mem: &'b Bump, doc: &'a LinearDoc<'a>) -> &'b FixedDoc<'b> {
    fn _eod<'a>(mem: &'a Bump) -> &'a FixedDoc<'a> {
        mem.alloc(FixedDoc::Eod)
    }

    fn _break<'a>(mem: &'a Bump, obj: &'a FixedObj<'a>, doc: &'a FixedDoc<'a>) -> &'a FixedDoc<'a> {
        mem.alloc(FixedDoc::Break(obj, doc))
    }

    fn _next<'a>(
        mem: &'a Bump,
        item: &'a FixedItem<'a>,
        comp: &'a FixedComp<'a>,
        obj: &'a FixedObj<'a>,
    ) -> &'a FixedObj<'a> {
        mem.alloc(FixedObj::Next(item, comp, obj))
    }

    fn _last<'a>(mem: &'a Bump, item: &'a FixedItem<'a>) -> &'a FixedObj<'a> {
        mem.alloc(FixedObj::Last(item))
    }

    fn _fix<'a>(mem: &'a Bump, fix: &'a FixedFix<'a>) -> &'a FixedItem<'a> {
        mem.alloc(FixedItem::Fix(fix))
    }

    fn _term<'a>(mem: &'a Bump, term: &'a FixedTerm<'a>) -> &'a FixedItem<'a> {
        mem.alloc(FixedItem::Term(term))
    }

    fn _null<'a>(mem: &'a Bump) -> &'a FixedTerm<'a> {
        mem.alloc(FixedTerm::Null)
    }

    fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a FixedTerm<'a> {
        mem.alloc(FixedTerm::Text(data))
    }

    fn _nest<'a>(mem: &'a Bump, term: &'a FixedTerm<'a>) -> &'a FixedTerm<'a> {
        mem.alloc(FixedTerm::Nest(term))
    }

    fn _pack<'a>(mem: &'a Bump, index: u64, term: &'a FixedTerm<'a>) -> &'a FixedTerm<'a> {
        mem.alloc(FixedTerm::Pack(index, term))
    }

    fn _comp<'a>(mem: &'a Bump, pad: bool) -> &'a FixedComp<'a> {
        mem.alloc(FixedComp::Comp(pad))
    }

    fn _grp<'a>(mem: &'a Bump, index: u64, comp: &'a FixedComp<'a>) -> &'a FixedComp<'a> {
        mem.alloc(FixedComp::Grp(index, comp))
    }

    fn _seq<'a>(mem: &'a Bump, index: u64, comp: &'a FixedComp<'a>) -> &'a FixedComp<'a> {
        mem.alloc(FixedComp::Seq(index, comp))
    }

    fn _fix_next<'a>(
        mem: &'a Bump,
        term: &'a FixedTerm<'a>,
        comp: &'a FixedComp<'a>,
        fix: &'a FixedFix<'a>,
    ) -> &'a FixedFix<'a> {
        mem.alloc(FixedFix::Next(term, comp, fix))
    }

    fn _fix_last<'a>(mem: &'a Bump, term: &'a FixedTerm<'a>) -> &'a FixedFix<'a> {
        mem.alloc(FixedFix::Last(term))
    }

    fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a LinearDoc<'a>) -> &'b FixedDoc<'b> {
        match doc {
            LinearDoc::Nil => _eod(mem),
            LinearDoc::Cons(obj, doc1) => _visit_obj(
                mem,
                obj,
                mem.alloc(move |mem, obj1| {
                    let doc2 = _visit_doc(mem, doc1);
                    _break(mem, obj1, doc2)
                }),
            ),
        }
    }

    fn _visit_obj<'b, 'a: 'b, R>(
        mem: &'b Bump,
        obj: &'a LinearObj<'a>,
        cont: &'b dyn Fn(&'b Bump, &'b FixedObj<'b>) -> R,
    ) -> R {
        match obj {
            LinearObj::Next(term, comp, obj1) => _visit_term(
                mem,
                term,
                mem.alloc(move |mem, term1| {
                    let (is_fixed, comp1) = _visit_comp(mem, comp);
                    if is_fixed {
                        _visit_fix(
                            mem,
                            obj1,
                            mem.alloc(move |mem, fix| _fix_next(mem, term1, comp1, fix)),
                            cont,
                        )
                    } else {
                        _visit_obj(
                            mem,
                            obj1,
                            compose(
                                mem,
                                cont,
                                mem.alloc(|mem, obj2| _next(mem, _term(mem, term1), comp1, obj2)),
                            ),
                        )
                    }
                }),
            ),
            LinearObj::Last(term) => _visit_term(
                mem,
                term,
                mem.alloc(|mem, term1| cont(mem, _last(mem, _term(mem, term1)))),
            ),
        }
    }

    fn _visit_fix<'b, 'a: 'b, R>(
        mem: &'b Bump,
        obj: &'a LinearObj<'a>,
        line: &'b dyn Fn(&'b Bump, &'b FixedFix<'b>) -> &'b FixedFix<'b>,
        cont: &'b dyn Fn(&'b Bump, &'b FixedObj<'b>) -> R,
    ) -> R {
        match obj {
            LinearObj::Next(term, comp, obj1) => _visit_term(
                mem,
                term,
                mem.alloc(move |mem, term1| {
                    let (is_fixed, comp1) = _visit_comp(mem, comp);
                    if is_fixed {
                        _visit_fix(
                            mem,
                            obj1,
                            compose(
                                mem,
                                line,
                                mem.alloc(move |mem, fix| _fix_next(mem, term1, comp1, fix)),
                            ),
                            cont,
                        )
                    } else {
                        _visit_obj(
                            mem,
                            obj1,
                            compose(
                                mem,
                                cont,
                                mem.alloc(|mem, obj2| {
                                    _next(
                                        mem,
                                        _fix(mem, line(mem, _fix_last(mem, term1))),
                                        comp1,
                                        obj2,
                                    )
                                }),
                            ),
                        )
                    }
                }),
            ),
            LinearObj::Last(term) => _visit_term(
                mem,
                term,
                mem.alloc(|mem, term1| {
                    cont(mem, _last(mem, _fix(mem, line(mem, _fix_last(mem, term1)))))
                }),
            ),
        }
    }

    fn _visit_term<'b, 'a: 'b, R>(
        mem: &'b Bump,
        term: &'a LinearTerm<'a>,
        cont: &'b dyn Fn(&'b Bump, &'b FixedTerm<'b>) -> R,
    ) -> R {
        match term {
            LinearTerm::Null => cont(mem, _null(mem)),
            LinearTerm::Text(data) => cont(mem, _text(mem, data)),
            LinearTerm::Nest(term1) => _visit_term(
                mem,
                term1,
                compose(mem, cont, mem.alloc(|mem, term2| _nest(mem, term2))),
            ),
            LinearTerm::Pack(index, term1) => _visit_term(
                mem,
                term1,
                compose(mem, cont, mem.alloc(|mem, term2| _pack(mem, *index, term2))),
            ),
        }
    }

    fn _visit_comp<'b, 'a: 'b>(
        mem: &'b Bump,
        comp: &'a LinearComp<'a>,
    ) -> (bool, &'b FixedComp<'b>) {
        match comp {
            LinearComp::Comp(attr) => (attr.fix, _comp(mem, attr.pad)),
            LinearComp::Grp(index, comp1) => {
                let (is_fixed, comp2) = _visit_comp(mem, comp1);
                (is_fixed, _grp(mem, *index, comp2))
            }
            LinearComp::Seq(index, comp1) => {
                let (is_fixed, comp2) = _visit_comp(mem, comp1);
                (is_fixed, _seq(mem, *index, comp2))
            }
        }
    }

    _visit_doc(mem, doc)
}
