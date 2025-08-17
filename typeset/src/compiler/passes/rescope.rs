//! Pass 9: DenullDoc → FinalDoc (rescope nest and pack)

use crate::compiler::types::{
    DenullDoc, DenullFix, DenullObj, DenullTerm, FinalDoc, FinalDocObj, FinalDocObjFix,
};
use crate::list::{self as _list, List};
use crate::util::compose;
use bumpalo::Bump;

#[derive(Debug, Copy, Clone)]
enum Prop {
    Nest,
    Pack(u64),
}

/// Rescope nest and pack
pub fn rescope<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b FinalDoc<'b> {
    fn _eod<'a>(mem: &'a Bump) -> &'a FinalDoc<'a> {
        mem.alloc(FinalDoc::Eod)
    }
    fn _empty<'a>(mem: &'a Bump, doc: &'a FinalDoc<'a>) -> &'a FinalDoc<'a> {
        mem.alloc(FinalDoc::Empty(doc))
    }
    fn _break<'a>(
        mem: &'a Bump,
        obj: &'a FinalDocObj<'a>,
        doc: &'a FinalDoc<'a>,
    ) -> &'a FinalDoc<'a> {
        mem.alloc(FinalDoc::Break(obj, doc))
    }
    fn _line<'a>(mem: &'a Bump, obj: &'a FinalDocObj<'a>) -> &'a FinalDoc<'a> {
        mem.alloc(FinalDoc::Line(obj))
    }
    fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a FinalDocObj<'a> {
        mem.alloc(FinalDocObj::Text(data))
    }
    fn _fix<'a>(mem: &'a Bump, fix: &'a FinalDocObjFix<'a>) -> &'a FinalDocObj<'a> {
        mem.alloc(FinalDocObj::Fix(fix))
    }
    fn _grp<'a>(mem: &'a Bump, obj: &'a FinalDocObj<'a>) -> &'a FinalDocObj<'a> {
        mem.alloc(FinalDocObj::Grp(obj))
    }
    fn _seq<'a>(mem: &'a Bump, obj: &'a FinalDocObj<'a>) -> &'a FinalDocObj<'a> {
        mem.alloc(FinalDocObj::Seq(obj))
    }
    fn _nest<'a>(mem: &'a Bump, obj: &'a FinalDocObj<'a>) -> &'a FinalDocObj<'a> {
        mem.alloc(FinalDocObj::Nest(obj))
    }
    fn _pack<'a>(mem: &'a Bump, index: u64, obj: &'a FinalDocObj<'a>) -> &'a FinalDocObj<'a> {
        mem.alloc(FinalDocObj::Pack(index, obj))
    }
    fn _comp<'a>(
        mem: &'a Bump,
        left: &'a FinalDocObj<'a>,
        right: &'a FinalDocObj<'a>,
        pad: bool,
    ) -> &'a FinalDocObj<'a> {
        mem.alloc(FinalDocObj::Comp(left, right, pad))
    }
    fn _fix_text<'a>(mem: &'a Bump, data: &'a str) -> &'a FinalDocObjFix<'a> {
        mem.alloc(FinalDocObjFix::Text(data))
    }
    fn _fix_comp<'a>(
        mem: &'a Bump,
        left: &'a FinalDocObjFix<'a>,
        right: &'a FinalDocObjFix<'a>,
        pad: bool,
    ) -> &'a FinalDocObjFix<'a> {
        mem.alloc(FinalDocObjFix::Comp(left, right, pad))
    }
    fn _prop_pack(index: u64) -> Prop {
        Prop::Pack(index)
    }
    fn _join_props<'b, 'a: 'b>(
        mem: &'b Bump,
        l: &'a List<'a, Prop>,
        r: &'a List<'a, Prop>,
    ) -> (&'b List<'b, Prop>, &'b List<'b, Prop>, &'b List<'b, Prop>) {
        fn _visit<'b, 'a: 'b>(
            mem: &'b Bump,
            l: &'a List<'a, Prop>,
            r: &'a List<'a, Prop>,
            c: &'a dyn Fn(&'b Bump, &'a List<'a, Prop>) -> &'a List<'a, Prop>,
        ) -> (&'b List<'b, Prop>, &'b List<'b, Prop>, &'b List<'b, Prop>) {
            match (l, r) {
                (List::Cons(_, Prop::Nest, l1), List::Cons(_, Prop::Nest, r1)) => {
                    let c1 = compose(
                        mem,
                        c,
                        mem.alloc(|mem, props| _list::cons(mem, Prop::Nest, props)),
                    );
                    _visit(mem, l1, r1, c1)
                }
                (
                    List::Cons(_, Prop::Pack(l_index), l1),
                    List::Cons(_, Prop::Pack(r_index), r1),
                ) => {
                    if l_index != r_index {
                        (l, r, c(mem, _list::nil(mem)))
                    } else {
                        let c1 = compose(
                            mem,
                            c,
                            mem.alloc(|mem, props| _list::cons(mem, _prop_pack(*l_index), props)),
                        );
                        _visit(mem, l1, r1, c1)
                    }
                }
                (_, _) => (l, r, c(mem, _list::nil(mem))),
            }
        }
        _visit(mem, l, r, mem.alloc(|_mem, props| props))
    }
    fn _apply_props<'b, 'a: 'b, R>(
        mem: &'b Bump,
        props: &'a List<'a, Prop>,
        term: &'a FinalDocObj<'a>,
        cont: &'b dyn Fn(&'b Bump, &'b FinalDocObj<'b>) -> R,
    ) -> R {
        match props {
            List::Nil => cont(mem, term),
            List::Cons(_, Prop::Nest, props1) => _apply_props(
                mem,
                props1,
                term,
                compose(mem, cont, mem.alloc(|mem, obj| _nest(mem, obj))),
            ),
            List::Cons(_, Prop::Pack(index), props1) => _apply_props(
                mem,
                props1,
                term,
                compose(mem, cont, mem.alloc(|mem, obj| _pack(mem, *index, obj))),
            ),
        }
    }
    fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b FinalDoc<'b> {
        match doc {
            DenullDoc::Eod => _eod(mem),
            DenullDoc::Empty(doc1) => {
                let doc2 = _visit_doc(mem, doc1);
                _empty(mem, doc2)
            }
            DenullDoc::Break(obj, doc1) => {
                let (props, obj1) = _visit_obj(mem, obj);
                _apply_props(
                    mem,
                    props,
                    obj1,
                    mem.alloc(move |mem, obj2| {
                        let doc2 = _visit_doc(mem, doc1);
                        _break(mem, obj2, doc2)
                    }),
                )
            }
            DenullDoc::Line(obj) => {
                let (props, obj1) = _visit_obj(mem, obj);
                _apply_props(mem, props, obj1, mem.alloc(|mem, obj2| _line(mem, obj2)))
            }
        }
    }
    fn _visit_obj<'b, 'a: 'b>(
        mem: &'b Bump,
        obj: &'a DenullObj<'a>,
    ) -> (&'b List<'b, Prop>, &'b FinalDocObj<'b>) {
        match obj {
            DenullObj::Term(term) => _visit_term(mem, term, mem.alloc(|_mem, props| props)),
            DenullObj::Fix(fix) => {
                let (props, fix1) = _visit_fix(mem, fix);
                (props, _fix(mem, fix1))
            }
            DenullObj::Grp(obj1) => {
                let (props, obj2) = _visit_obj(mem, obj1);
                (props, _grp(mem, obj2))
            }
            DenullObj::Seq(obj1) => {
                let (props, obj2) = _visit_obj(mem, obj1);
                (props, _seq(mem, obj2))
            }
            DenullObj::Comp(left, right, pad) => {
                let (l_props, left1) = _visit_obj(mem, left);
                let (r_props, right1) = _visit_obj(mem, right);
                let (l_props1, r_props1, c_props) = _join_props(mem, l_props, r_props);
                _apply_props(
                    mem,
                    l_props1,
                    left1,
                    mem.alloc(move |mem, left2| {
                        _apply_props(
                            mem,
                            r_props1,
                            right1,
                            mem.alloc(move |mem, right2| {
                                (c_props, _comp(mem, left2, right2, *pad))
                            }),
                        )
                    }),
                )
            }
        }
    }
    fn _visit_fix<'b, 'a: 'b>(
        mem: &'b Bump,
        fix: &'a DenullFix<'a>,
    ) -> (&'b List<'b, Prop>, &'b FinalDocObjFix<'b>) {
        match fix {
            DenullFix::Term(term) => _visit_fix_term(mem, term, mem.alloc(|_mem, props| props)),
            DenullFix::Comp(left, right, pad) => {
                let (l_props, left1) = _visit_fix(mem, left);
                let (_r_props, right1) = _visit_fix(mem, right);
                (l_props, _fix_comp(mem, left1, right1, *pad))
            }
        }
    }
    fn _visit_term<'b, 'a: 'b>(
        mem: &'b Bump,
        term: &'a DenullTerm<'a>,
        result: &'b dyn Fn(&'b Bump, &'b List<'b, Prop>) -> &'b List<'b, Prop>,
    ) -> (&'b List<'b, Prop>, &'b FinalDocObj<'b>) {
        match term {
            DenullTerm::Text(data) => (result(mem, _list::nil(mem)), _text(mem, data)),
            DenullTerm::Nest(term1) => {
                let result1 = compose(
                    mem,
                    result,
                    mem.alloc(|mem, props| _list::cons(mem, Prop::Nest, props)),
                );
                _visit_term(mem, term1, result1)
            }
            DenullTerm::Pack(index, term1) => {
                let result1 = compose(
                    mem,
                    result,
                    mem.alloc(|mem, props| _list::cons(mem, _prop_pack(*index), props)),
                );
                _visit_term(mem, term1, result1)
            }
        }
    }
    fn _visit_fix_term<'b, 'a: 'b>(
        mem: &'b Bump,
        term: &'a DenullTerm<'a>,
        result: &'b dyn Fn(&'b Bump, &'b List<'b, Prop>) -> &'b List<'b, Prop>,
    ) -> (&'b List<'b, Prop>, &'b FinalDocObjFix<'b>) {
        match term {
            DenullTerm::Text(data) => (result(mem, _list::nil(mem)), _fix_text(mem, data)),
            DenullTerm::Nest(term1) => {
                let result1 = compose(
                    mem,
                    result,
                    mem.alloc(|mem, props| _list::cons(mem, Prop::Nest, props)),
                );
                _visit_fix_term(mem, term1, result1)
            }
            DenullTerm::Pack(index, term1) => {
                let result1 = compose(
                    mem,
                    result,
                    mem.alloc(|mem, props| _list::cons(mem, _prop_pack(*index), props)),
                );
                _visit_fix_term(mem, term1, result1)
            }
        }
    }
    _visit_doc(mem, doc)
}
