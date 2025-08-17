//! Pass 6: RebuildDoc → DenullDoc (remove null identities)

use crate::compiler::types::{
    DenullDoc, DenullFix, DenullObj, DenullTerm, RebuildDoc, RebuildFix, RebuildObj, RebuildTerm,
};
use crate::util::compose;
use bumpalo::Bump;

/// Remove null identities
pub fn denull<'b, 'a: 'b>(mem: &'b Bump, doc: &'a RebuildDoc<'a>) -> &'b DenullDoc<'b> {
    fn _eod<'a>(mem: &'a Bump) -> &'a DenullDoc<'a> {
        mem.alloc(DenullDoc::Eod)
    }
    fn _line<'a>(mem: &'a Bump, obj: &'a DenullObj<'a>) -> &'a DenullDoc<'a> {
        mem.alloc(DenullDoc::Line(obj))
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
    fn _fix_term<'a>(mem: &'a Bump, term: &'a DenullTerm<'a>) -> &'a DenullFix<'a> {
        mem.alloc(DenullFix::Term(term))
    }
    fn _fix_comp<'a>(
        mem: &'a Bump,
        left: &'a DenullFix<'a>,
        right: &'a DenullFix<'a>,
        pad: bool,
    ) -> &'a DenullFix<'a> {
        mem.alloc(DenullFix::Comp(left, right, pad))
    }
    fn _text<'a>(mem: &'a Bump, data: &'a str) -> &'a DenullTerm<'a> {
        mem.alloc(DenullTerm::Text(data))
    }
    fn _nest<'a>(mem: &'a Bump, term: &'a DenullTerm<'a>) -> &'a DenullTerm<'a> {
        mem.alloc(DenullTerm::Nest(term))
    }
    fn _pack<'a>(mem: &'a Bump, index: u64, term: &'a DenullTerm<'a>) -> &'a DenullTerm<'a> {
        mem.alloc(DenullTerm::Pack(index, term))
    }
    fn _visit_doc<'b, 'a: 'b, R>(
        mem: &'b Bump,
        doc: &'a RebuildDoc<'a>,
        none: &'b dyn Fn(&'b Bump) -> R,
        some: &'b dyn Fn(&'b Bump, &'b DenullDoc<'b>) -> R,
    ) -> R {
        match doc {
            RebuildDoc::Eod => none(mem),
            RebuildDoc::Break(obj, doc1) => _visit_obj(
                mem,
                obj,
                mem.alloc(|mem| {
                    _visit_doc(
                        mem,
                        doc1,
                        mem.alloc(|mem| some(mem, _eod(mem))),
                        mem.alloc(|mem, doc2| some(mem, _empty(mem, doc2))),
                    )
                }),
                mem.alloc(move |mem, obj1| {
                    _visit_doc(
                        mem,
                        doc1,
                        mem.alloc(move |mem| some(mem, _line(mem, obj1))),
                        mem.alloc(|mem, doc2| some(mem, _break(mem, obj1, doc2))),
                    )
                }),
                mem.alloc(move |mem, _pad, obj1| {
                    _visit_doc(
                        mem,
                        doc1,
                        mem.alloc(move |mem| some(mem, _line(mem, obj1))),
                        mem.alloc(|mem, doc2| some(mem, _break(mem, obj1, doc2))),
                    )
                }),
            ),
        }
    }
    fn _visit_obj<'b, 'a: 'b, R>(
        mem: &'b Bump,
        obj: &'a RebuildObj<'a>,
        last_none: &'b dyn Fn(&'b Bump) -> R,
        last_some: &'b dyn Fn(&'b Bump, &'b DenullObj<'b>) -> R,
        next_none: &'b dyn Fn(&'b Bump, bool, &'b DenullObj<'b>) -> R,
    ) -> R {
        match obj {
            RebuildObj::Term(term) => _visit_term(
                mem,
                term,
                last_none,
                compose(mem, last_some, mem.alloc(|mem, term1| _term(mem, term1))),
            ),
            RebuildObj::Fix(fix) => _visit_fix(
                mem,
                fix,
                last_none,
                compose(mem, last_some, mem.alloc(|mem, fix1| _fix(mem, fix1))),
                mem.alloc(|mem, _comp, fix1| last_some(mem, _fix(mem, fix1))),
            ),
            RebuildObj::Grp(obj1) => _visit_obj(
                mem,
                obj1,
                last_none,
                compose(mem, last_some, mem.alloc(|mem, obj2| _grp(mem, obj2))),
                mem.alloc(|mem, _pad, obj2| last_some(mem, _grp(mem, obj2))),
            ),
            RebuildObj::Seq(obj1) => _visit_obj(
                mem,
                obj1,
                last_none,
                compose(mem, last_some, mem.alloc(|mem, obj2| _seq(mem, obj2))),
                mem.alloc(|mem, _pad, obj2| last_some(mem, _seq(mem, obj2))),
            ),
            RebuildObj::Comp(left, right, l_pad) => _visit_obj(
                mem,
                left,
                mem.alloc(|mem| {
                    _visit_obj(
                        mem,
                        right,
                        last_none,
                        mem.alloc(|mem, right1| next_none(mem, *l_pad, right1)),
                        mem.alloc(|mem, r_pad, right1| {
                            let pad = *l_pad || r_pad;
                            next_none(mem, pad, right1)
                        }),
                    )
                }),
                mem.alloc(move |mem, left1| {
                    _visit_obj(
                        mem,
                        right,
                        mem.alloc(move |mem| last_some(mem, left1)),
                        mem.alloc(|mem, right1| last_some(mem, _comp(mem, left1, right1, *l_pad))),
                        mem.alloc(|mem, r_pad, right1| {
                            let pad = *l_pad || r_pad;
                            last_some(mem, _comp(mem, left1, right1, pad))
                        }),
                    )
                }),
                mem.alloc(|_mem, _pad, _left1| unreachable!("Invariant")),
            ),
        }
    }
    fn _visit_fix<'b, 'a: 'b, R>(
        mem: &'b Bump,
        fix: &'a RebuildFix<'a>,
        last_none: &'b dyn Fn(&'b Bump) -> R,
        last_some: &'b dyn Fn(&'b Bump, &'b DenullFix<'b>) -> R,
        next_none: &'b dyn Fn(&'b Bump, bool, &'b DenullFix<'b>) -> R,
    ) -> R {
        match fix {
            RebuildFix::Term(term) => _visit_term(
                mem,
                term,
                last_none,
                compose(
                    mem,
                    last_some,
                    mem.alloc(|mem, term1| _fix_term(mem, term1)),
                ),
            ),
            RebuildFix::Comp(left, right, l_pad) => _visit_fix(
                mem,
                left,
                mem.alloc(|mem| {
                    _visit_fix(
                        mem,
                        right,
                        last_none,
                        mem.alloc(|mem, right1| next_none(mem, *l_pad, right1)),
                        mem.alloc(|mem, r_pad, right1| {
                            let pad = *l_pad || r_pad;
                            next_none(mem, pad, right1)
                        }),
                    )
                }),
                mem.alloc(move |mem, left1| {
                    _visit_fix(
                        mem,
                        right,
                        mem.alloc(move |mem| last_some(mem, left1)),
                        mem.alloc(|mem, right1| {
                            last_some(mem, _fix_comp(mem, left1, right1, *l_pad))
                        }),
                        mem.alloc(|mem, r_pad, right1| {
                            let pad = *l_pad || r_pad;
                            last_some(mem, _fix_comp(mem, left1, right1, pad))
                        }),
                    )
                }),
                mem.alloc(|_mem, _pad, _left1| unreachable!("Invariant")),
            ),
        }
    }
    fn _visit_term<'b, 'a: 'b, R>(
        mem: &'b Bump,
        term: &'a RebuildTerm<'a>,
        none: &'b dyn Fn(&'b Bump) -> R,
        some: &'b dyn Fn(&'b Bump, &'b DenullTerm<'b>) -> R,
    ) -> R {
        match term {
            RebuildTerm::Null => none(mem),
            RebuildTerm::Text(data) => {
                if data.is_empty() {
                    none(mem)
                } else {
                    some(mem, _text(mem, data))
                }
            }
            RebuildTerm::Nest(term1) => _visit_term(
                mem,
                term1,
                none,
                compose(mem, some, mem.alloc(|mem, term2| _nest(mem, term2))),
            ),
            RebuildTerm::Pack(index, term1) => _visit_term(
                mem,
                term1,
                none,
                compose(mem, some, mem.alloc(|mem, term2| _pack(mem, *index, term2))),
            ),
        }
    }
    _visit_doc(
        mem,
        doc,
        mem.alloc(|mem| _eod(mem)),
        mem.alloc(|_mem, doc1| doc1),
    )
}
