//! Pass 8: DenullDoc → DenullDoc (reassociate after grp/seq removals)

use crate::compiler::types::{DenullDoc, DenullFix, DenullObj, DenullTerm};
use crate::util::compose;
use bumpalo::Bump;

/// Reassociate after grp and seq removals
pub fn reassociate<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
    fn _eod<'a>(mem: &'a Bump) -> &'a DenullDoc<'a> {
        mem.alloc(DenullDoc::Eod)
    }
    fn _empty<'a>(mem: &'a Bump, doc: &'a DenullDoc<'a>) -> &'a DenullDoc<'a> {
        mem.alloc(DenullDoc::Empty(doc))
    }
    fn _break<'a>(
        mem: &'a Bump,
        obj: &'a DenullObj<'a>,
        doc: &'a DenullDoc<'a>,
    ) -> &'a DenullDoc<'a> {
        mem.alloc(DenullDoc::Break(obj, doc))
    }
    fn _line<'a>(mem: &'a Bump, obj: &'a DenullObj<'a>) -> &'a DenullDoc<'a> {
        mem.alloc(DenullDoc::Line(obj))
    }
    fn _term<'a>(mem: &'a Bump, term: &'a DenullTerm<'a>) -> &'a DenullObj<'a> {
        mem.alloc(DenullObj::Term(term))
    }
    fn _fix<'a>(mem: &'a Bump, fix: &'a DenullFix<'a>) -> &'a DenullObj<'a> {
        mem.alloc(DenullObj::Fix(fix))
    }
    fn _grp<'a>(mem: &'a Bump, obj: &'a DenullObj<'a>) -> &'a DenullObj<'a> {
        mem.alloc(DenullObj::Grp(obj))
    }
    fn _seq<'a>(mem: &'a Bump, obj: &'a DenullObj<'a>) -> &'a DenullObj<'a> {
        mem.alloc(DenullObj::Seq(obj))
    }
    fn _comp<'a>(
        mem: &'a Bump,
        left: &'a DenullObj<'a>,
        right: &'a DenullObj<'a>,
        pad: bool,
    ) -> &'a DenullObj<'a> {
        mem.alloc(DenullObj::Comp(left, right, pad))
    }
    fn __comp<'a>(
        mem: &'a Bump,
        pad: bool,
        right: &'a DenullObj<'a>,
        left: &'a DenullObj<'a>,
    ) -> &'a DenullObj<'a> {
        _comp(mem, left, right, pad)
    }
    fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
        match doc {
            DenullDoc::Eod => _eod(mem),
            DenullDoc::Empty(doc1) => {
                let doc2 = _visit_doc(mem, doc1);
                _empty(mem, doc2)
            }
            DenullDoc::Break(obj, doc1) => {
                let partial = mem.alloc(move |_mem, obj1| obj1);
                _visit_obj(
                    mem,
                    obj,
                    partial,
                    mem.alloc(move |mem, obj2| {
                        let doc2 = _visit_doc(mem, doc1);
                        _break(mem, obj2, doc2)
                    }),
                )
            }
            DenullDoc::Line(obj) => {
                let partial = mem.alloc(|_mem, obj1| obj1);
                _visit_obj(mem, obj, partial, mem.alloc(|mem, obj2| _line(mem, obj2)))
            }
        }
    }
    fn _visit_obj<'b, 'a: 'b, R>(
        mem: &'b Bump,
        obj: &'a DenullObj<'a>,
        partial: &'b dyn Fn(&'b Bump, &'b DenullObj<'b>) -> &'b DenullObj<'b>,
        cont: &'b dyn Fn(&'b Bump, &'b DenullObj<'b>) -> R,
    ) -> R {
        match obj {
            DenullObj::Term(term) => cont(mem, partial(mem, _term(mem, term))),
            DenullObj::Fix(fix) => cont(mem, partial(mem, _fix(mem, fix))),
            DenullObj::Grp(obj1) => _visit_obj(
                mem,
                obj1,
                mem.alloc(|_mem, obj2| obj2),
                compose(
                    mem,
                    cont,
                    compose(mem, partial, mem.alloc(|mem, obj3| _grp(mem, obj3))),
                ),
            ),
            DenullObj::Seq(obj1) => _visit_obj(
                mem,
                obj1,
                mem.alloc(|_mem, obj2| obj2),
                compose(
                    mem,
                    cont,
                    compose(mem, partial, mem.alloc(|mem, obj3| _seq(mem, obj3))),
                ),
            ),
            DenullObj::Comp(left, right, pad) => _visit_obj(
                mem,
                right,
                partial,
                mem.alloc(move |mem, result| {
                    _visit_obj(
                        mem,
                        left,
                        mem.alloc(move |mem, obj1| __comp(mem, *pad, result, obj1)),
                        cont,
                    )
                }),
            ),
        }
    }
    _visit_doc(mem, doc)
}
