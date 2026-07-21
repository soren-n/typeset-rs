//! Pass 7: DenullDoc → DenullDoc (remove grp and seq identities)
//!
//! Two sub-passes each fold every object bottom-up, tracking a `Count` of the
//! compositions in a subtree so a grp/seq wrapping fewer than two of them can
//! be dropped as an identity. The folds were already direct-style but recursed
//! on the native stack (Comp branches), aborting on deep inputs. Here each
//! `_visit_obj` runs as a descend/ascend trampoline over a heap-allocated frame
//! stack, and the doc spines are plain loops.

use crate::compiler::types::{DenullDoc, DenullFix, DenullObj};
use bumpalo::Bump;

#[derive(Debug, Copy, Clone)]
enum Count {
    Zero,
    One,
    Many,
}

fn _add(left: Count, right: Count) -> Count {
    match (left, right) {
        (Count::Zero, _) => right,
        (_, Count::Zero) => left,
        (Count::Many, _) | (_, Count::Many) | (Count::One, Count::One) => Count::Many,
    }
}

/// A doc-spine element with a tail (terminals are handled separately).
enum DocItem<'b> {
    Empty,
    Break(&'b DenullObj<'b>),
}

/// Frames for the seq-elimination object trampoline.
enum SeqFrame<'b, 'a> {
    Grp,
    Seq {
        node_under: bool,
    },
    CompLeft {
        right: &'a DenullObj<'a>,
        pad: bool,
        under: bool,
    },
    CompRight {
        left: (Count, &'b DenullObj<'b>),
        pad: bool,
    },
}

/// Frames for the grp-elimination object trampoline.
enum GrpFrame<'b, 'a> {
    Grp {
        node_in_head: bool,
    },
    Seq,
    CompLeft {
        right: &'a DenullObj<'a>,
        pad: bool,
    },
    CompRight {
        left: (Count, &'b DenullObj<'b>),
        pad: bool,
    },
}

/// Remove grp and seq identities
pub fn identities<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
    let doc1 = _elim_seqs(mem, doc);
    _elim_grps(mem, doc1)
}

/// Walk a linear doc spine, mapping each object with `obj_fn`, then fold the
/// collected items back onto the Eod/Line terminal.
fn _elim_spine<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a DenullDoc<'a>,
    obj_fn: impl Fn(&'b Bump, &'a DenullObj<'a>) -> &'b DenullObj<'b>,
) -> &'b DenullDoc<'b> {
    let mut items: Vec<DocItem<'b>> = Vec::new();
    let mut cur = doc;
    let terminal: &'b DenullDoc<'b> = loop {
        match cur {
            DenullDoc::Eod => break mem.alloc(DenullDoc::Eod),
            DenullDoc::Line(obj) => break mem.alloc(DenullDoc::Line(obj_fn(mem, obj))),
            DenullDoc::Empty(doc1) => {
                items.push(DocItem::Empty);
                cur = doc1;
            }
            DenullDoc::Break(obj, doc1) => {
                items.push(DocItem::Break(obj_fn(mem, obj)));
                cur = doc1;
            }
        }
    };
    let mut result = terminal;
    for item in items.iter().rev() {
        result = match item {
            DocItem::Empty => mem.alloc(DenullDoc::Empty(result)),
            DocItem::Break(obj) => mem.alloc(DenullDoc::Break(obj, result)),
        };
    }
    result
}

fn _elim_seqs<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
    _elim_spine(mem, doc, |mem, obj| _visit_obj_seqs(mem, obj, false).1)
}

/// Bottom-up fold eliminating seq wrappers that group fewer than two
/// compositions, and absorbing directly nested seqs.
fn _visit_obj_seqs<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: &'a DenullObj<'a>,
    under_seq: bool,
) -> (Count, &'b DenullObj<'b>) {
    let mut stack: Vec<SeqFrame<'b, 'a>> = Vec::new();
    let mut cur = obj;
    let mut under = under_seq;
    'machine: loop {
        let mut val: (Count, &'b DenullObj<'b>) = loop {
            match cur {
                DenullObj::Term(term) | DenullObj::Fix(DenullFix::Term(term)) => {
                    break (Count::Zero, mem.alloc(DenullObj::Term(term)));
                }
                DenullObj::Fix(fix) => break (Count::Zero, mem.alloc(DenullObj::Fix(fix))),
                DenullObj::Grp(obj1) => {
                    stack.push(SeqFrame::Grp);
                    cur = obj1;
                    under = false;
                }
                DenullObj::Seq(obj1) => {
                    stack.push(SeqFrame::Seq { node_under: under });
                    cur = obj1;
                    under = true;
                }
                DenullObj::Comp(left, right, pad) => {
                    stack.push(SeqFrame::CompLeft {
                        right,
                        pad: *pad,
                        under,
                    });
                    cur = left;
                }
            }
        };
        loop {
            match stack.pop() {
                None => return val,
                Some(SeqFrame::Grp) => val = (Count::Zero, mem.alloc(DenullObj::Grp(val.1))),
                Some(SeqFrame::Seq { node_under }) => {
                    val = if node_under {
                        // A seq directly under a seq is absorbed.
                        val
                    } else {
                        match val.0 {
                            Count::Zero | Count::One => val,
                            Count::Many => (Count::Many, mem.alloc(DenullObj::Seq(val.1))),
                        }
                    };
                }
                Some(SeqFrame::CompLeft {
                    right,
                    pad,
                    under: u,
                }) => {
                    stack.push(SeqFrame::CompRight { left: val, pad });
                    cur = right;
                    under = u;
                    continue 'machine;
                }
                Some(SeqFrame::CompRight { left, pad }) => {
                    let count = _add(Count::One, _add(left.0, val.0));
                    val = (count, mem.alloc(DenullObj::Comp(left.1, val.1, pad)));
                }
            }
        }
    }
}

fn _elim_grps<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
    _elim_spine(mem, doc, |mem, obj| _visit_obj_grps(mem, obj, true).1)
}

/// Bottom-up fold eliminating grp wrappers that group fewer than two
/// compositions, and absorbing grps at the head of their enclosing group.
fn _visit_obj_grps<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: &'a DenullObj<'a>,
    in_head: bool,
) -> (Count, &'b DenullObj<'b>) {
    let mut stack: Vec<GrpFrame<'b, 'a>> = Vec::new();
    let mut cur = obj;
    let mut head = in_head;
    'machine: loop {
        let mut val: (Count, &'b DenullObj<'b>) = loop {
            match cur {
                DenullObj::Term(term) | DenullObj::Fix(DenullFix::Term(term)) => {
                    break (Count::Zero, mem.alloc(DenullObj::Term(term)));
                }
                DenullObj::Fix(fix) => break (Count::Zero, mem.alloc(DenullObj::Fix(fix))),
                DenullObj::Grp(obj1) => {
                    stack.push(GrpFrame::Grp { node_in_head: head });
                    cur = obj1;
                    // The child inherits the node's in_head flag.
                }
                DenullObj::Seq(obj1) => {
                    stack.push(GrpFrame::Seq);
                    cur = obj1;
                    head = false;
                }
                DenullObj::Comp(left, right, pad) => {
                    stack.push(GrpFrame::CompLeft { right, pad: *pad });
                    cur = left;
                }
            }
        };
        loop {
            match stack.pop() {
                None => return val,
                Some(GrpFrame::Grp { node_in_head }) => {
                    val = if node_in_head {
                        // A grp at the head of its group is absorbed.
                        val
                    } else {
                        match val.0 {
                            Count::Zero => (Count::Zero, val.1),
                            Count::One | Count::Many => {
                                (Count::Zero, mem.alloc(DenullObj::Grp(val.1)))
                            }
                        }
                    };
                }
                Some(GrpFrame::Seq) => val = (val.0, mem.alloc(DenullObj::Seq(val.1))),
                Some(GrpFrame::CompLeft { right, pad }) => {
                    // The right operand is never in head position; the left
                    // already descended with the node's in_head flag.
                    stack.push(GrpFrame::CompRight { left: val, pad });
                    cur = right;
                    head = false;
                    continue 'machine;
                }
                Some(GrpFrame::CompRight { left, pad }) => {
                    let count = _add(Count::One, _add(left.0, val.0));
                    val = (count, mem.alloc(DenullObj::Comp(left.1, val.1, pad)));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::DenullTerm;

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    fn term<'b>(mem: &'b Bump, s: &'b str) -> &'b DenullObj<'b> {
        mem.alloc(DenullObj::Term(mem.alloc(DenullTerm::Text(s))))
    }

    #[test]
    fn identities_preserve_deep_comp_chain() {
        let mem = Bump::new();
        // Left-nested comp chain, no grp/seq: identities rebuild it unchanged.
        let mut obj: &DenullObj = term(&mem, "a");
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Comp(obj, term(&mem, "b"), false));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = identities(&mem, doc);
        let DenullDoc::Line(result) = out else {
            panic!("expected a line")
        };
        let mut count = 0usize;
        let mut cur: &DenullObj = result;
        while let DenullObj::Comp(left, _right, _pad) = cur {
            count += 1;
            cur = left;
        }
        assert!(matches!(cur, DenullObj::Term(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn identities_handle_deep_seq_nesting() {
        let mem = Bump::new();
        // Deep seq nesting exercises the seq trampoline's unary frames.
        let mut obj: &DenullObj = term(&mem, "x");
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Seq(obj));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        // Should not overflow; a seq around a single term collapses away.
        let out = identities(&mem, doc);
        assert!(matches!(out, DenullDoc::Line(_)));
    }

    #[test]
    fn identities_handle_deep_grp_nesting() {
        let mem = Bump::new();
        let mut obj: &DenullObj = term(&mem, "x");
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Grp(obj));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = identities(&mem, doc);
        assert!(matches!(out, DenullDoc::Line(_)));
    }

    #[test]
    fn identities_handle_long_doc_spine() {
        let mem = Bump::new();
        let mut doc: &DenullDoc = mem.alloc(DenullDoc::Eod);
        for _ in 0..DEEP {
            doc = mem.alloc(DenullDoc::Break(term(&mem, "x"), doc));
        }
        let out = identities(&mem, doc);
        let mut count = 0usize;
        let mut cur = out;
        while let DenullDoc::Break(_, rest) = cur {
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, DenullDoc::Eod));
        assert_eq!(count, DEEP);
    }
}
