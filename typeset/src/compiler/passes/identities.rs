//! Pass 7: DenullDoc → DenullDoc (remove grp and seq identities)

use crate::compiler::types::{DenullDoc, DenullFix, DenullObj, DenullTerm};
use bumpalo::Bump;

#[derive(Debug, Copy, Clone)]
enum Count {
    Zero,
    One,
    Many,
}

/// Remove grp and seq identities
pub fn identities<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
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
    fn _add(left: Count, right: Count) -> Count {
        match (left, right) {
            (Count::Zero, _) => right,
            (_, Count::Zero) => left,
            (Count::Many, _) | (_, Count::Many) | (Count::One, Count::One) => Count::Many,
        }
    }
    fn _elim_seqs<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
        fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
            match doc {
                DenullDoc::Eod => _eod(mem),
                DenullDoc::Empty(doc1) => {
                    let doc2 = _visit_doc(mem, doc1);
                    _empty(mem, doc2)
                }
                DenullDoc::Break(obj, doc1) => {
                    let (_count, obj1) = _visit_obj(mem, obj, false);
                    let doc2 = _visit_doc(mem, doc1);
                    _break(mem, obj1, doc2)
                }
                DenullDoc::Line(obj) => {
                    let (_count, obj1) = _visit_obj(mem, obj, false);
                    _line(mem, obj1)
                }
            }
        }
        fn _visit_obj<'b, 'a: 'b>(
            mem: &'b Bump,
            obj: &'a DenullObj<'a>,
            under_seq: bool,
        ) -> (Count, &'b DenullObj<'b>) {
            match obj {
                DenullObj::Term(term) | DenullObj::Fix(DenullFix::Term(term)) => {
                    (Count::Zero, _term(mem, term))
                }
                DenullObj::Fix(fix) => (Count::Zero, _fix(mem, fix)),
                DenullObj::Grp(obj1) => {
                    let (_count, obj2) = _visit_obj(mem, obj1, false);
                    (Count::Zero, _grp(mem, obj2))
                }
                DenullObj::Seq(obj1) => {
                    if under_seq {
                        _visit_obj(mem, obj1, true)
                    } else {
                        let (count, obj2) = _visit_obj(mem, obj1, true);
                        match count {
                            Count::Zero | Count::One => (count, obj2),
                            Count::Many => (Count::Many, _seq(mem, obj2)),
                        }
                    }
                }
                DenullObj::Comp(left, right, pad) => {
                    let (l_count, left1) = _visit_obj(mem, left, under_seq);
                    let (r_count, right1) = _visit_obj(mem, right, under_seq);
                    let count = _add(Count::One, _add(l_count, r_count));
                    (count, _comp(mem, left1, right1, *pad))
                }
            }
        }
        _visit_doc(mem, doc)
    }
    fn _elim_grps<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
        fn _visit_doc<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
            match doc {
                DenullDoc::Eod => _eod(mem),
                DenullDoc::Empty(doc1) => {
                    let doc2 = _visit_doc(mem, doc1);
                    _empty(mem, doc2)
                }
                DenullDoc::Break(obj, doc1) => {
                    let (_count, obj1) = _visit_obj(mem, obj, true);
                    let doc2 = _visit_doc(mem, doc1);
                    _break(mem, obj1, doc2)
                }
                DenullDoc::Line(obj) => {
                    let (_count, obj1) = _visit_obj(mem, obj, true);
                    _line(mem, obj1)
                }
            }
        }
        fn _visit_obj<'b, 'a: 'b>(
            mem: &'b Bump,
            obj: &'a DenullObj<'a>,
            in_head: bool,
        ) -> (Count, &'b DenullObj<'b>) {
            match obj {
                DenullObj::Term(term) | DenullObj::Fix(DenullFix::Term(term)) => {
                    (Count::Zero, _term(mem, term))
                }
                DenullObj::Fix(fix) => (Count::Zero, _fix(mem, fix)),
                DenullObj::Grp(obj1) => {
                    if in_head {
                        _visit_obj(mem, obj1, true)
                    } else {
                        let (count, obj2) = _visit_obj(mem, obj1, false);
                        match count {
                            Count::Zero => (Count::Zero, obj2),
                            Count::One | Count::Many => (Count::Zero, _grp(mem, obj2)),
                        }
                    }
                }
                DenullObj::Seq(obj1) => {
                    let (count, obj2) = _visit_obj(mem, obj1, false);
                    (count, _seq(mem, obj2))
                }
                DenullObj::Comp(left, right, pad) => {
                    let (l_count, left1) = _visit_obj(mem, left, in_head);
                    let (r_count, right1) = _visit_obj(mem, right, false);
                    let count = _add(Count::One, _add(l_count, r_count));
                    (count, _comp(mem, left1, right1, *pad))
                }
            }
        }
        _visit_doc(mem, doc)
    }
    let doc1 = _elim_seqs(mem, doc);
    _elim_grps(mem, doc1)
}
