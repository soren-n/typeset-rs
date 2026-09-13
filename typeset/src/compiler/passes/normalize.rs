//! normalize: DenullDoc → DenullDoc (normalize the composition algebra)
//!
//! Runs the three grp/seq normalization folds back-to-back, in this order (the
//! order matters — the rules are not confluent):
//! 1. seq elimination — [`elim_seqs`]: drop seq wrappers grouping fewer than
//!    two compositions, and absorb a seq nested directly under a seq.
//! 2. grp elimination — [`elim_grps`]: drop grp wrappers grouping fewer than
//!    two compositions, and absorb a grp at the head of its enclosing group.
//! 3. reassociation — [`reassoc`]: right-associate composition trees,
//!    reassociating inside each Grp/Seq boundary independently.
//!
//! The document is a flat postorder arena, so each fold is two plain loops: a
//! backward pass distributing the inherited context (parents precede children
//! in reverse order), and a forward pass computing the bottom-up rebuild
//! (children precede parents in forward order).

use super::denull::{DObjId, DenullDoc};
use crate::compiler::types::{Arena, DenullTerm, Fix, IdVec, Obj, Pad};

/// The object and fixed-object node types this pass folds.
type DObj<'a> = Obj<DenullTerm<'a>>;
type DFix<'a> = Fix<DenullTerm<'a>>;

/// Normalize the grp/seq composition algebra.
pub fn normalize(doc: DenullDoc) -> DenullDoc {
    reassoc(elim_grps(elim_seqs(doc)))
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

/// A leaf in the elimination folds: a term contributes no compositions, and a
/// fix that holds a single term unwraps to that term.
fn leaf_obj<'a>(objs: &mut Arena<DObj<'a>>, fixes: &Arena<DFix<'a>>, node: DObj<'a>) -> DObjId<'a> {
    match node {
        Obj::Fix(fix) => match fixes[fix] {
            Fix::Term(term) => objs.push(Obj::Term(term)),
            Fix::Comp(..) => objs.push(Obj::Fix(fix)),
        },
        other => objs.push(other),
    }
}

// Fold 1: seq elimination
// -----------------------

/// Drop seq wrappers grouping fewer than two compositions, and absorb a seq
/// nested directly under a seq.
fn elim_seqs(doc: DenullDoc) -> DenullDoc {
    let n = doc.objs.len();
    // Backward pass: whether each node sits directly under a seq. Roots start
    // outside any seq; comps pass the flag through, grp resets it.
    let mut under: IdVec<DObj, bool> = IdVec::filled(false, n);
    for (i, node) in doc.objs.iter().rev() {
        match *node {
            Obj::Grp(c) => under[c] = false,
            Obj::Seq(c) => under[c] = true,
            Obj::Comp(l, r, _) => {
                under[l] = under[i];
                under[r] = under[i];
            }
            Obj::Term(_) | Obj::Fix(_) => {}
        }
    }
    // Forward pass: rebuild bottom-up, tracking each subtree's composition
    // count. The count passes through a seq wrapper unchanged.
    let mut objs: Arena<DObj> = Arena::with_capacity(n);
    let mut out: IdVec<DObj, DObjId> = IdVec::with_capacity(n);
    let mut count: IdVec<DObj, Count> = IdVec::with_capacity(n);
    for (i, node) in doc.objs.into_iter() {
        let (c, id) = match node {
            Obj::Term(_) | Obj::Fix(_) => (Count::Zero, leaf_obj(&mut objs, &doc.fixes, node)),
            Obj::Grp(c1) => (Count::Zero, objs.push(Obj::Grp(out[c1]))),
            Obj::Seq(c1) => {
                let (cc, cid) = (count[c1], out[c1]);
                if under[i] {
                    // Directly under a seq: absorb the wrapper.
                    (cc, cid)
                } else {
                    match cc {
                        Count::Zero | Count::One => (cc, cid),
                        Count::Many => (Count::Many, objs.push(Obj::Seq(cid))),
                    }
                }
            }
            Obj::Comp(l, r, pad) => (
                add(Count::One, add(count[l], count[r])),
                objs.push(Obj::Comp(out[l], out[r], pad)),
            ),
        };
        count.push(c);
        out.push(id);
    }
    DenullDoc {
        lines: doc
            .lines
            .iter()
            .map(|root| root.map(|id| out[id]))
            .collect(),
        objs,
        fixes: doc.fixes,
        props: doc.props,
    }
}

// Fold 2: grp elimination
// -----------------------

/// Drop grp wrappers grouping fewer than one composition, and absorb a grp at
/// the head of its enclosing group.
fn elim_grps(doc: DenullDoc) -> DenullDoc {
    let n = doc.objs.len();
    // Backward pass: whether each node is in head position of its enclosing
    // group. Roots are; a comp's left operand inherits, its right does not;
    // seq resets.
    let mut head: IdVec<DObj, bool> = IdVec::filled(false, n);
    for root in doc.lines.iter().flatten() {
        head[*root] = true;
    }
    for (i, node) in doc.objs.iter().rev() {
        match *node {
            Obj::Grp(c) => head[c] = head[i],
            Obj::Seq(c) => head[c] = false,
            Obj::Comp(l, r, _) => {
                head[l] = head[i];
                head[r] = false;
            }
            Obj::Term(_) | Obj::Fix(_) => {}
        }
    }
    // Forward pass: rebuild bottom-up. A grp contributes no composition to its
    // enclosing count, so the count resets to Zero wherever a wrapper is kept
    // or dropped for grouping too little; an absorbed head grp passes its
    // count through.
    let mut objs: Arena<DObj> = Arena::with_capacity(n);
    let mut out: IdVec<DObj, DObjId> = IdVec::with_capacity(n);
    let mut count: IdVec<DObj, Count> = IdVec::with_capacity(n);
    for (i, node) in doc.objs.into_iter() {
        let (c, id) = match node {
            Obj::Term(_) | Obj::Fix(_) => (Count::Zero, leaf_obj(&mut objs, &doc.fixes, node)),
            Obj::Seq(c1) => (count[c1], objs.push(Obj::Seq(out[c1]))),
            Obj::Grp(c1) => {
                let (cc, cid) = (count[c1], out[c1]);
                if head[i] {
                    // At the head of the enclosing group: absorb the wrapper.
                    (cc, cid)
                } else {
                    match cc {
                        Count::Zero => (Count::Zero, cid),
                        Count::One | Count::Many => (Count::Zero, objs.push(Obj::Grp(cid))),
                    }
                }
            }
            Obj::Comp(l, r, pad) => (
                add(Count::One, add(count[l], count[r])),
                objs.push(Obj::Comp(out[l], out[r], pad)),
            ),
        };
        count.push(c);
        out.push(id);
    }
    DenullDoc {
        lines: doc
            .lines
            .iter()
            .map(|root| root.map(|id| out[id]))
            .collect(),
        objs,
        fixes: doc.fixes,
        props: doc.props,
    }
}

// Fold 3: reassociation
// ---------------------

/// Right-associate composition trees (e.g. `Comp(Comp(a, b, p1), c, p2)`
/// becomes `Comp(a, Comp(b, c, p2), p1)`), treating Term/Fix/Grp/Seq as atoms
/// and reassociating inside each Grp/Seq boundary independently.
///
/// Each comp subtree is threaded as a chain of atoms with the pads between
/// them: an atom is its own one-element chain, and a comp links its left
/// chain's tail to its right chain's head in O(1). At each boundary (a grp or
/// seq child, or a row root) the chain is materialized as a right-nested
/// composition spine.
fn reassoc(doc: DenullDoc) -> DenullDoc {
    let n = doc.objs.len();
    let mut objs: Arena<DObj> = Arena::with_capacity(n);
    // Chain state per input node: `head`/`tail` are input ids of the chain's
    // endpoints, `atom_out` is an atom's output id (`None` for a comp, which
    // is never an atom), and `next[tail] = (pad, head-of-next)` links
    // adjacent atoms.
    let mut atom_out: IdVec<DObj, Option<DObjId>> = IdVec::with_capacity(n);
    let mut head: IdVec<DObj, DObjId> = IdVec::with_capacity(n);
    let mut tail: IdVec<DObj, DObjId> = IdVec::with_capacity(n);
    let mut next: IdVec<DObj, Option<(Pad, DObjId)>> = IdVec::filled(None, n);

    // Scratch for `materialize`, reused across calls: one materialization
    // runs per grp/seq boundary and per row, so fresh vectors here would be
    // per-node allocation churn.
    let mut atoms: Vec<DObjId> = Vec::new();
    let mut pads: Vec<Pad> = Vec::new();

    fn materialize<'a>(
        objs: &mut Arena<DObj<'a>>,
        atom_out: &IdVec<DObj<'a>, Option<DObjId<'a>>>,
        next: &IdVec<DObj<'a>, Option<(Pad, DObjId<'a>)>>,
        start: DObjId<'a>,
        atoms: &mut Vec<DObjId<'a>>,
        pads: &mut Vec<Pad>,
    ) -> DObjId<'a> {
        // Collect the chain's atoms (output ids) and the pads between them.
        atoms.clear();
        pads.clear();
        let mut cur = start;
        loop {
            atoms.push(atom_out[cur].expect("chain elements are atoms"));
            match next[cur] {
                Some((pad, nxt)) => {
                    pads.push(pad);
                    cur = nxt;
                }
                None => break,
            }
        }
        // Rebuild right-nested: Comp(a0, Comp(a1, ..., p1), p0).
        let mut result = *atoms.last().expect("a chain has at least one atom");
        for k in (0..pads.len()).rev() {
            result = objs.push(Obj::Comp(atoms[k], result, pads[k]));
        }
        result
    }

    for (i, node) in doc.objs.into_iter() {
        match node {
            Obj::Term(_) | Obj::Fix(_) => {
                atom_out.push(Some(objs.push(node)));
                head.push(i);
                tail.push(i);
            }
            Obj::Grp(c) => {
                let spine =
                    materialize(&mut objs, &atom_out, &next, head[c], &mut atoms, &mut pads);
                atom_out.push(Some(objs.push(Obj::Grp(spine))));
                head.push(i);
                tail.push(i);
            }
            Obj::Seq(c) => {
                let spine =
                    materialize(&mut objs, &atom_out, &next, head[c], &mut atoms, &mut pads);
                atom_out.push(Some(objs.push(Obj::Seq(spine))));
                head.push(i);
                tail.push(i);
            }
            Obj::Comp(l, r, pad) => {
                next[tail[l]] = Some((pad, head[r]));
                atom_out.push(None);
                head.push(head[l]);
                tail.push(tail[r]);
            }
        }
    }

    let lines = doc
        .lines
        .iter()
        .map(|root| {
            root.map(|id| materialize(&mut objs, &atom_out, &next, head[id], &mut atoms, &mut pads))
        })
        .collect();

    DenullDoc {
        lines,
        objs,
        fixes: doc.fixes,
        props: doc.props,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::Range;

    /// Far past where a native-stack recursion could survive; with flat arenas
    /// the folds are plain loops, so this guards sizing behavior only.
    const DEEP: usize = 50_000;

    fn term(text: &'static str) -> DObj<'static> {
        Obj::Term(DenullTerm {
            props: Range::EMPTY,
            text,
        })
    }

    fn line_doc<'a>(objs: Arena<DObj<'a>>, root: DObjId<'a>) -> DenullDoc<'a> {
        DenullDoc {
            lines: vec![Some(root)],
            objs,
            fixes: Arena::new(),
            props: Vec::new(),
        }
    }

    #[test]
    fn normalize_right_associates_deep_left_nested_comp() {
        // Left-nested comp chain, no grp/seq: normalization rebuilds it
        // right-nested (a right spine of DEEP compositions).
        let mut objs: Arena<DObj> = Arena::new();
        let mut cur = objs.push(term("a"));
        for _ in 0..DEEP {
            let right = objs.push(term("b"));
            cur = objs.push(Obj::Comp(cur, right, Pad::Unpadded));
        }
        let out = normalize(line_doc(objs, cur));
        let [Some(root)] = out.lines[..] else {
            panic!("expected a line");
        };
        let mut count = 0usize;
        let mut walk = root;
        while let Obj::Comp(_left, right, _pad) = out.objs[walk] {
            count += 1;
            walk = right;
        }
        assert!(matches!(out.objs[walk], Obj::Term(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn normalize_collapses_deep_seq_nesting() {
        // Deep seq nesting over a single term collapses away entirely.
        let mut objs: Arena<DObj> = Arena::new();
        let mut cur = objs.push(term("x"));
        for _ in 0..DEEP {
            cur = objs.push(Obj::Seq(cur));
        }
        let out = normalize(line_doc(objs, cur));
        let [Some(root)] = out.lines[..] else {
            panic!("expected a line");
        };
        assert!(matches!(out.objs[root], Obj::Term(_)));
    }

    #[test]
    fn normalize_collapses_deep_grp_nesting() {
        let mut objs: Arena<DObj> = Arena::new();
        let mut cur = objs.push(term("x"));
        for _ in 0..DEEP {
            cur = objs.push(Obj::Grp(cur));
        }
        let out = normalize(line_doc(objs, cur));
        let [Some(root)] = out.lines[..] else {
            panic!("expected a line");
        };
        assert!(matches!(out.objs[root], Obj::Term(_)));
    }

    #[test]
    fn normalize_handles_long_doc_spine() {
        let mut objs: Arena<DObj> = Arena::new();
        let mut lines = Vec::new();
        for _ in 0..DEEP {
            lines.push(Some(objs.push(term("x"))));
        }
        let doc = DenullDoc {
            lines,
            objs,
            fixes: Arena::new(),
            props: Vec::new(),
        };
        let out = normalize(doc);
        assert_eq!(out.lines.len(), DEEP);
    }

    #[test]
    fn seq_of_one_comp_is_dropped_but_two_kept() {
        // A seq grouping a single composition collapses; a seq grouping two or
        // more is kept.
        let mut objs: Arena<DObj> = Arena::new();
        let a = objs.push(term("a"));
        let b = objs.push(term("b"));
        let comp = objs.push(Obj::Comp(a, b, Pad::Unpadded));
        let one = objs.push(Obj::Seq(comp));
        let out = elim_seqs(line_doc(objs, one));
        let [Some(root)] = out.lines[..] else {
            panic!("expected a line");
        };
        assert!(
            matches!(out.objs[root], Obj::Comp(..)),
            "seq of one comp dropped"
        );

        let mut objs: Arena<DObj> = Arena::new();
        let a = objs.push(term("a"));
        let b = objs.push(term("b"));
        let ab = objs.push(Obj::Comp(a, b, Pad::Unpadded));
        let c = objs.push(term("c"));
        let abc = objs.push(Obj::Comp(ab, c, Pad::Unpadded));
        let two = objs.push(Obj::Seq(abc));
        let out = elim_seqs(line_doc(objs, two));
        let [Some(root)] = out.lines[..] else {
            panic!("expected a line");
        };
        assert!(
            matches!(out.objs[root], Obj::Seq(_)),
            "seq of two comps kept"
        );
    }
}
