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

use crate::compiler::types::{Arena, DObjId, DenullDoc, DenullFix, DenullObj, DenullRow, IdVec};

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

/// Remap a spine row through the fold's old-id → new-id table.
fn map_row<'a>(row: DenullRow<'a>, out: &IdVec<DenullObj<'a>, DObjId<'a>>) -> DenullRow<'a> {
    match row {
        DenullRow::Empty => DenullRow::Empty,
        DenullRow::Break(id) => DenullRow::Break(out[id]),
        DenullRow::Line(id) => DenullRow::Line(out[id]),
    }
}

/// A leaf in the elimination folds: a term contributes no compositions, and a
/// fix that holds a single term unwraps to that term.
fn leaf_obj<'a>(
    objs: &mut Arena<DenullObj<'a>>,
    fixes: &Arena<DenullFix<'a>>,
    node: DenullObj<'a>,
) -> DObjId<'a> {
    match node {
        DenullObj::Fix(fix) => match fixes[fix] {
            DenullFix::Term(term) => objs.push(DenullObj::Term(term)),
            DenullFix::Comp(..) => objs.push(DenullObj::Fix(fix)),
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
    let mut under: IdVec<DenullObj, bool> = IdVec::filled(false, n);
    for (i, node) in doc.objs.iter().rev() {
        match *node {
            DenullObj::Grp(c) => under[c] = false,
            DenullObj::Seq(c) => under[c] = true,
            DenullObj::Comp(l, r, _) => {
                under[l] = under[i];
                under[r] = under[i];
            }
            DenullObj::Term(_) | DenullObj::Fix(_) => {}
        }
    }
    // Forward pass: rebuild bottom-up, tracking each subtree's composition
    // count. The count passes through a seq wrapper unchanged.
    let mut objs: Arena<DenullObj> = Arena::with_capacity(n);
    let mut out: IdVec<DenullObj, DObjId> = IdVec::with_capacity(n);
    let mut count: IdVec<DenullObj, Count> = IdVec::with_capacity(n);
    for (i, node) in doc.objs.into_iter() {
        let (c, id) = match node {
            DenullObj::Term(_) | DenullObj::Fix(_) => {
                (Count::Zero, leaf_obj(&mut objs, &doc.fixes, node))
            }
            DenullObj::Grp(c1) => (Count::Zero, objs.push(DenullObj::Grp(out[c1]))),
            DenullObj::Seq(c1) => {
                let (cc, cid) = (count[c1], out[c1]);
                if under[i] {
                    // Directly under a seq: absorb the wrapper.
                    (cc, cid)
                } else {
                    match cc {
                        Count::Zero | Count::One => (cc, cid),
                        Count::Many => (Count::Many, objs.push(DenullObj::Seq(cid))),
                    }
                }
            }
            DenullObj::Comp(l, r, pad) => (
                add(Count::One, add(count[l], count[r])),
                objs.push(DenullObj::Comp(out[l], out[r], pad)),
            ),
        };
        count.push(c);
        out.push(id);
    }
    DenullDoc {
        rows: doc.rows.into_iter().map(|r| map_row(r, &out)).collect(),
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
    let mut head: IdVec<DenullObj, bool> = IdVec::filled(false, n);
    for row in &doc.rows {
        match row {
            DenullRow::Break(id) | DenullRow::Line(id) => head[*id] = true,
            DenullRow::Empty => {}
        }
    }
    for (i, node) in doc.objs.iter().rev() {
        match *node {
            DenullObj::Grp(c) => head[c] = head[i],
            DenullObj::Seq(c) => head[c] = false,
            DenullObj::Comp(l, r, _) => {
                head[l] = head[i];
                head[r] = false;
            }
            DenullObj::Term(_) | DenullObj::Fix(_) => {}
        }
    }
    // Forward pass: rebuild bottom-up. A grp contributes no composition to its
    // enclosing count, so the count resets to Zero wherever a wrapper is kept
    // or dropped for grouping too little; an absorbed head grp passes its
    // count through.
    let mut objs: Arena<DenullObj> = Arena::with_capacity(n);
    let mut out: IdVec<DenullObj, DObjId> = IdVec::with_capacity(n);
    let mut count: IdVec<DenullObj, Count> = IdVec::with_capacity(n);
    for (i, node) in doc.objs.into_iter() {
        let (c, id) = match node {
            DenullObj::Term(_) | DenullObj::Fix(_) => {
                (Count::Zero, leaf_obj(&mut objs, &doc.fixes, node))
            }
            DenullObj::Seq(c1) => (count[c1], objs.push(DenullObj::Seq(out[c1]))),
            DenullObj::Grp(c1) => {
                let (cc, cid) = (count[c1], out[c1]);
                if head[i] {
                    // At the head of the enclosing group: absorb the wrapper.
                    (cc, cid)
                } else {
                    match cc {
                        Count::Zero => (Count::Zero, cid),
                        Count::One | Count::Many => (Count::Zero, objs.push(DenullObj::Grp(cid))),
                    }
                }
            }
            DenullObj::Comp(l, r, pad) => (
                add(Count::One, add(count[l], count[r])),
                objs.push(DenullObj::Comp(out[l], out[r], pad)),
            ),
        };
        count.push(c);
        out.push(id);
    }
    DenullDoc {
        rows: doc.rows.into_iter().map(|r| map_row(r, &out)).collect(),
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
    let mut objs: Arena<DenullObj> = Arena::with_capacity(n);
    // Chain state per input node: `head`/`tail` are input ids of the chain's
    // endpoints, `atom_out` is an atom's output id (`None` for a comp, which
    // is never an atom), and `next[tail] = (pad, head-of-next)` links
    // adjacent atoms.
    let mut atom_out: IdVec<DenullObj, Option<DObjId>> = IdVec::with_capacity(n);
    let mut head: IdVec<DenullObj, DObjId> = IdVec::with_capacity(n);
    let mut tail: IdVec<DenullObj, DObjId> = IdVec::with_capacity(n);
    let mut next: IdVec<DenullObj, Option<(bool, DObjId)>> = IdVec::filled(None, n);

    // Scratch for `materialize`, reused across calls: one materialization
    // runs per grp/seq boundary and per row, so fresh vectors here would be
    // per-node allocation churn.
    let mut atoms: Vec<DObjId> = Vec::new();
    let mut pads: Vec<bool> = Vec::new();

    fn materialize<'a>(
        objs: &mut Arena<DenullObj<'a>>,
        atom_out: &IdVec<DenullObj<'a>, Option<DObjId<'a>>>,
        next: &IdVec<DenullObj<'a>, Option<(bool, DObjId<'a>)>>,
        start: DObjId<'a>,
        atoms: &mut Vec<DObjId<'a>>,
        pads: &mut Vec<bool>,
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
            result = objs.push(DenullObj::Comp(atoms[k], result, pads[k]));
        }
        result
    }

    for (i, node) in doc.objs.into_iter() {
        match node {
            DenullObj::Term(_) | DenullObj::Fix(_) => {
                atom_out.push(Some(objs.push(node)));
                head.push(i);
                tail.push(i);
            }
            DenullObj::Grp(c) => {
                let spine =
                    materialize(&mut objs, &atom_out, &next, head[c], &mut atoms, &mut pads);
                atom_out.push(Some(objs.push(DenullObj::Grp(spine))));
                head.push(i);
                tail.push(i);
            }
            DenullObj::Seq(c) => {
                let spine =
                    materialize(&mut objs, &atom_out, &next, head[c], &mut atoms, &mut pads);
                atom_out.push(Some(objs.push(DenullObj::Seq(spine))));
                head.push(i);
                tail.push(i);
            }
            DenullObj::Comp(l, r, pad) => {
                next[tail[l]] = Some((pad, head[r]));
                atom_out.push(None);
                head.push(head[l]);
                tail.push(tail[r]);
            }
        }
    }

    let rows = doc
        .rows
        .into_iter()
        .map(|row| match row {
            DenullRow::Empty => DenullRow::Empty,
            DenullRow::Break(id) => DenullRow::Break(materialize(
                &mut objs, &atom_out, &next, head[id], &mut atoms, &mut pads,
            )),
            DenullRow::Line(id) => DenullRow::Line(materialize(
                &mut objs, &atom_out, &next, head[id], &mut atoms, &mut pads,
            )),
        })
        .collect();

    DenullDoc {
        rows,
        objs,
        fixes: doc.fixes,
        props: doc.props,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{DenullTerm, Range};

    /// Far past where a native-stack recursion could survive; with flat arenas
    /// the folds are plain loops, so this guards sizing behavior only.
    const DEEP: usize = 50_000;

    fn term(text: &'static str) -> DenullObj<'static> {
        DenullObj::Term(DenullTerm {
            props: Range::EMPTY,
            text,
        })
    }

    fn line_doc<'a>(objs: Arena<DenullObj<'a>>, root: DObjId<'a>) -> DenullDoc<'a> {
        DenullDoc {
            rows: vec![DenullRow::Line(root)],
            objs,
            fixes: Arena::new(),
            props: Vec::new(),
        }
    }

    #[test]
    fn normalize_right_associates_deep_left_nested_comp() {
        // Left-nested comp chain, no grp/seq: normalization rebuilds it
        // right-nested (a right spine of DEEP compositions).
        let mut objs: Arena<DenullObj> = Arena::new();
        let mut cur = objs.push(term("a"));
        for _ in 0..DEEP {
            let right = objs.push(term("b"));
            cur = objs.push(DenullObj::Comp(cur, right, false));
        }
        let out = normalize(line_doc(objs, cur));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        let mut count = 0usize;
        let mut walk = root;
        while let DenullObj::Comp(_left, right, _pad) = out.objs[walk] {
            count += 1;
            walk = right;
        }
        assert!(matches!(out.objs[walk], DenullObj::Term(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn normalize_collapses_deep_seq_nesting() {
        // Deep seq nesting over a single term collapses away entirely.
        let mut objs: Arena<DenullObj> = Arena::new();
        let mut cur = objs.push(term("x"));
        for _ in 0..DEEP {
            cur = objs.push(DenullObj::Seq(cur));
        }
        let out = normalize(line_doc(objs, cur));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        assert!(matches!(out.objs[root], DenullObj::Term(_)));
    }

    #[test]
    fn normalize_collapses_deep_grp_nesting() {
        let mut objs: Arena<DenullObj> = Arena::new();
        let mut cur = objs.push(term("x"));
        for _ in 0..DEEP {
            cur = objs.push(DenullObj::Grp(cur));
        }
        let out = normalize(line_doc(objs, cur));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        assert!(matches!(out.objs[root], DenullObj::Term(_)));
    }

    #[test]
    fn normalize_handles_long_doc_spine() {
        let mut objs: Arena<DenullObj> = Arena::new();
        let mut rows: Vec<DenullRow> = Vec::new();
        for _ in 0..DEEP {
            rows.push(DenullRow::Break(objs.push(term("x"))));
        }
        let doc = DenullDoc {
            rows,
            objs,
            fixes: Arena::new(),
            props: Vec::new(),
        };
        let out = normalize(doc);
        assert_eq!(out.rows.len(), DEEP);
    }

    #[test]
    fn seq_of_one_comp_is_dropped_but_two_kept() {
        // A seq grouping a single composition collapses; a seq grouping two or
        // more is kept.
        let mut objs: Arena<DenullObj> = Arena::new();
        let a = objs.push(term("a"));
        let b = objs.push(term("b"));
        let comp = objs.push(DenullObj::Comp(a, b, false));
        let one = objs.push(DenullObj::Seq(comp));
        let out = elim_seqs(line_doc(objs, one));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        assert!(
            matches!(out.objs[root], DenullObj::Comp(..)),
            "seq of one comp dropped"
        );

        let mut objs: Arena<DenullObj> = Arena::new();
        let a = objs.push(term("a"));
        let b = objs.push(term("b"));
        let ab = objs.push(DenullObj::Comp(a, b, false));
        let c = objs.push(term("c"));
        let abc = objs.push(DenullObj::Comp(ab, c, false));
        let two = objs.push(DenullObj::Seq(abc));
        let out = elim_seqs(line_doc(objs, two));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        assert!(
            matches!(out.objs[root], DenullObj::Seq(_)),
            "seq of two comps kept"
        );
    }
}
