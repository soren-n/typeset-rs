//! Pass 7: DenullDoc → DenullDoc (normalize the composition algebra)
//!
//! Runs the three grp/seq normalization folds back-to-back over each line's
//! object, in this order (the order matters — the rules are not confluent):
//! 1. [`elim_seqs`]  — drop seq wrappers grouping fewer than two compositions,
//!    and absorb a seq nested directly under a seq.
//! 2. [`elim_grps`]  — drop grp wrappers grouping fewer than two compositions,
//!    and absorb a grp at the head of its enclosing group.
//! 3. [`reassoc`]    — right-associate composition trees, reassociating inside
//!    each Grp/Seq boundary independently.
//!
//! All three are bottom-up folds over the same `DenullObj` shape, so they share
//! one spine walk ([`map_denull_spine`]) and one arena: each line's object is
//! run through `reassoc ∘ elim_grps ∘ elim_seqs`. Composing per-object is
//! equivalent to running the folds as three separate whole-document passes,
//! since the spine walk maps each line independently.
//!
//! Each fold recurses on the native stack in its original direct-style form and
//! aborted on deep inputs; here each object visitor is a descend/ascend
//! trampoline over a heap-allocated frame stack, and the doc spine is a plain
//! loop.

use super::walk::map_denull_spine;
use crate::compiler::types::{DenullDoc, DenullFix, DenullObj};
use bumpalo::Bump;

/// Normalize the grp/seq composition algebra.
pub fn normalize<'b, 'a: 'b>(mem: &'b Bump, doc: &'a DenullDoc<'a>) -> &'b DenullDoc<'b> {
    map_denull_spine(mem, doc, |mem, obj| {
        let seqs = visit_obj_seqs(mem, obj, false).1;
        let grps = visit_obj_grps(mem, seqs, true).1;
        reassoc_obj(mem, grps)
    })
}

// Composition-count monoid
// ------------------------
// `elim_seqs`/`elim_grps` keep a grp/seq only when it groups two or more
// compositions, so each fold tracks how many compositions a subtree contains,
// saturating at `Many`.

#[derive(Debug, Copy, Clone)]
enum Count {
    Zero,
    One,
    Many,
}

fn add(left: Count, right: Count) -> Count {
    match (left, right) {
        (Count::Zero, _) => right,
        (_, Count::Zero) => left,
        (Count::Many, _) | (_, Count::Many) | (Count::One, Count::One) => Count::Many,
    }
}

// Fold 1: seq elimination
// -----------------------

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

/// Bottom-up fold eliminating seq wrappers that group fewer than two
/// compositions, and absorbing directly nested seqs.
fn visit_obj_seqs<'b, 'a: 'b>(
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
                    let count = add(Count::One, add(left.0, val.0));
                    val = (count, mem.alloc(DenullObj::Comp(left.1, val.1, pad)));
                }
            }
        }
    }
}

// Fold 2: grp elimination
// -----------------------

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

/// Bottom-up fold eliminating grp wrappers that group fewer than two
/// compositions, and absorbing grps at the head of their enclosing group.
fn visit_obj_grps<'b, 'a: 'b>(
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
                    let count = add(Count::One, add(left.0, val.0));
                    val = (count, mem.alloc(DenullObj::Comp(left.1, val.1, pad)));
                }
            }
        }
    }
}

// Fold 3: reassociation
// ---------------------
// Right-associates composition trees (e.g. `Comp(Comp(a, b, p1), c, p2)`
// becomes `Comp(a, Comp(b, c, p2), p1)`), treating Term/Fix/Grp/Seq as atoms
// and reassociating inside each Grp/Seq independently. The original expressed
// this with two nested continuations (a `partial` left-wrapper and a `cont`);
// here the object walk is a descend/ascend trampoline: `partial` is a small
// enum applied at each leaf, and `cont` is a heap-allocated frame stack.

/// A pending left-wrapper applied to a reassociated object at a leaf.
#[derive(Copy, Clone)]
enum Partial<'b> {
    /// Identity: pass the object through unchanged.
    Id,
    /// Wrap the object as the left operand of a composition with `right`.
    Comp { right: &'b DenullObj<'b>, pad: bool },
}

/// A pending continuation frame (the defunctionalized `cont`).
enum Frame<'b, 'a> {
    /// Wrap the ascending value in a Grp, then apply the saved partial.
    Grp(Partial<'b>),
    /// Wrap the ascending value in a Seq, then apply the saved partial.
    Seq(Partial<'b>),
    /// The left operand of a composition, still to be reassociated with the
    /// ascending value (the reassociated right operand) as its `right`.
    Comp { left: &'a DenullObj<'a>, pad: bool },
}

/// Reassociates a single object, right-associating its compositions.
fn reassoc_obj<'b, 'a: 'b>(mem: &'b Bump, obj: &'a DenullObj<'a>) -> &'b DenullObj<'b> {
    let mut stack: Vec<Frame<'b, 'a>> = Vec::new();
    let mut cur = obj;
    let mut partial = Partial::Id;
    'machine: loop {
        // Descend, visiting a composition's right operand first, until a leaf
        // produces an ascending value.
        let mut value: &'b DenullObj<'b> = loop {
            match cur {
                DenullObj::Term(term) => {
                    break apply_partial(mem, partial, mem.alloc(DenullObj::Term(term)));
                }
                DenullObj::Fix(fix) => {
                    break apply_partial(mem, partial, mem.alloc(DenullObj::Fix(fix)));
                }
                DenullObj::Grp(obj1) => {
                    // Grp is a boundary: reassociate its contents afresh.
                    stack.push(Frame::Grp(partial));
                    partial = Partial::Id;
                    cur = obj1;
                }
                DenullObj::Seq(obj1) => {
                    stack.push(Frame::Seq(partial));
                    partial = Partial::Id;
                    cur = obj1;
                }
                DenullObj::Comp(left, right, pad) => {
                    stack.push(Frame::Comp { left, pad: *pad });
                    cur = right;
                }
            }
        };
        // Ascend, applying frames until we must descend a Comp's left operand.
        loop {
            match stack.pop() {
                None => return value,
                Some(Frame::Grp(p)) => {
                    value = apply_partial(mem, p, mem.alloc(DenullObj::Grp(value)));
                }
                Some(Frame::Seq(p)) => {
                    value = apply_partial(mem, p, mem.alloc(DenullObj::Seq(value)));
                }
                Some(Frame::Comp { left, pad }) => {
                    cur = left;
                    partial = Partial::Comp { right: value, pad };
                    continue 'machine;
                }
            }
        }
    }
}

/// Applies a pending left-wrapper to a reassociated object.
fn apply_partial<'b>(
    mem: &'b Bump,
    partial: Partial<'b>,
    obj: &'b DenullObj<'b>,
) -> &'b DenullObj<'b> {
    match partial {
        Partial::Id => obj,
        Partial::Comp { right, pad } => mem.alloc(DenullObj::Comp(obj, right, pad)),
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
    fn normalize_right_associates_deep_left_nested_comp() {
        let mem = Bump::new();
        // Left-nested comp chain, no grp/seq: normalization rebuilds it
        // right-nested (a right spine of DEEP compositions).
        let mut obj: &DenullObj = term(&mem, "a");
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Comp(obj, term(&mem, "b"), false));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = normalize(&mem, doc);
        let DenullDoc::Line(result) = out else {
            panic!("expected a line")
        };
        let mut count = 0usize;
        let mut cur: &DenullObj = result;
        while let DenullObj::Comp(_left, right, _pad) = cur {
            count += 1;
            cur = right;
        }
        assert!(matches!(cur, DenullObj::Term(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn normalize_handles_deep_seq_nesting() {
        let mem = Bump::new();
        // Deep seq nesting exercises the seq trampoline's unary frames; a seq
        // around a single term collapses away.
        let mut obj: &DenullObj = term(&mem, "x");
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Seq(obj));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = normalize(&mem, doc);
        assert!(matches!(out, DenullDoc::Line(_)));
    }

    #[test]
    fn normalize_handles_deep_grp_nesting() {
        let mem = Bump::new();
        let mut obj: &DenullObj = term(&mem, "x");
        for _ in 0..DEEP {
            obj = mem.alloc(DenullObj::Grp(obj));
        }
        let doc: &DenullDoc = mem.alloc(DenullDoc::Line(obj));
        let out = normalize(&mem, doc);
        assert!(matches!(out, DenullDoc::Line(_)));
    }

    #[test]
    fn normalize_handles_long_doc_spine() {
        let mem = Bump::new();
        let mut doc: &DenullDoc = mem.alloc(DenullDoc::Eod);
        for _ in 0..DEEP {
            doc = mem.alloc(DenullDoc::Break(term(&mem, "x"), doc));
        }
        let out = normalize(&mem, doc);
        let mut count = 0usize;
        let mut cur = out;
        while let DenullDoc::Break(_, rest) = cur {
            count += 1;
            cur = rest;
        }
        assert!(matches!(cur, DenullDoc::Eod));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn seq_of_one_comp_is_dropped_but_two_kept() {
        // A seq grouping a single composition collapses; a seq grouping two or
        // more is kept.
        let mem = Bump::new();
        let one = mem.alloc(DenullObj::Seq(mem.alloc(DenullObj::Comp(
            term(&mem, "a"),
            term(&mem, "b"),
            false,
        ))));
        let out = visit_obj_seqs(&mem, one, false).1;
        assert!(
            matches!(out, DenullObj::Comp(..)),
            "seq of one comp dropped"
        );

        let two = mem.alloc(DenullObj::Seq(mem.alloc(DenullObj::Comp(
            mem.alloc(DenullObj::Comp(term(&mem, "a"), term(&mem, "b"), false)),
            term(&mem, "c"),
            false,
        ))));
        let out = visit_obj_seqs(&mem, two, false).1;
        assert!(matches!(out, DenullObj::Seq(_)), "seq of two comps kept");
    }
}
