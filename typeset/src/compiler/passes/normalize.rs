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
//! (children precede parents in forward order). No frame stacks anywhere.

use crate::compiler::types::{DObjId, DenullDoc, DenullFix, DenullObj, DenullRow};

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

/// Append an arena node and return its index.
fn push<'a>(objs: &mut Vec<DenullObj<'a>>, node: DenullObj<'a>) -> DObjId {
    let id = objs.len() as DObjId;
    objs.push(node);
    id
}

/// Remap a spine row through the fold's old-id → new-id table.
fn map_row(row: DenullRow, out: &[DObjId]) -> DenullRow {
    match row {
        DenullRow::Empty => DenullRow::Empty,
        DenullRow::Break(id) => DenullRow::Break(out[id as usize]),
        DenullRow::Line(id) => DenullRow::Line(out[id as usize]),
    }
}

/// A leaf in the elimination folds: a term contributes no compositions, and a
/// fix that holds a single term unwraps to that term.
fn leaf_obj<'a>(
    objs: &mut Vec<DenullObj<'a>>,
    fixes: &[DenullFix<'a>],
    node: DenullObj<'a>,
) -> DObjId {
    match node {
        DenullObj::Fix(fix) => match &fixes[fix as usize] {
            DenullFix::Term(term) => push(objs, DenullObj::Term(term.clone())),
            DenullFix::Comp(..) => push(objs, DenullObj::Fix(fix)),
        },
        other => push(objs, other),
    }
}

// Fold 1: seq elimination
// -----------------------

/// Drop seq wrappers grouping fewer than two compositions, and absorb a seq
/// nested directly under a seq.
fn elim_seqs(doc: DenullDoc) -> DenullDoc {
    // Backward pass: whether each node sits directly under a seq. Roots start
    // outside any seq; comps pass the flag through, grp resets it.
    let mut under = vec![false; doc.objs.len()];
    for i in (0..doc.objs.len()).rev() {
        match doc.objs[i] {
            DenullObj::Grp(c) => under[c as usize] = false,
            DenullObj::Seq(c) => under[c as usize] = true,
            DenullObj::Comp(l, r, _) => {
                under[l as usize] = under[i];
                under[r as usize] = under[i];
            }
            DenullObj::Term(_) | DenullObj::Fix(_) => {}
        }
    }
    // Forward pass: rebuild bottom-up, tracking each subtree's composition
    // count. The count passes through a seq wrapper unchanged.
    let mut objs: Vec<DenullObj> = Vec::with_capacity(doc.objs.len());
    let mut out: Vec<DObjId> = Vec::with_capacity(doc.objs.len());
    let mut count: Vec<Count> = Vec::with_capacity(doc.objs.len());
    for (i, node) in doc.objs.into_iter().enumerate() {
        let (c, id) = match node {
            DenullObj::Term(_) | DenullObj::Fix(_) => {
                (Count::Zero, leaf_obj(&mut objs, &doc.fixes, node))
            }
            DenullObj::Grp(c1) => (
                Count::Zero,
                push(&mut objs, DenullObj::Grp(out[c1 as usize])),
            ),
            DenullObj::Seq(c1) => {
                let (cc, cid) = (count[c1 as usize], out[c1 as usize]);
                if under[i] {
                    // Directly under a seq: absorb the wrapper.
                    (cc, cid)
                } else {
                    match cc {
                        Count::Zero | Count::One => (cc, cid),
                        Count::Many => (Count::Many, push(&mut objs, DenullObj::Seq(cid))),
                    }
                }
            }
            DenullObj::Comp(l, r, pad) => (
                add(Count::One, add(count[l as usize], count[r as usize])),
                push(
                    &mut objs,
                    DenullObj::Comp(out[l as usize], out[r as usize], pad),
                ),
            ),
        };
        count.push(c);
        out.push(id);
    }
    DenullDoc {
        rows: doc.rows.into_iter().map(|r| map_row(r, &out)).collect(),
        objs,
        fixes: doc.fixes,
    }
}

// Fold 2: grp elimination
// -----------------------

/// Drop grp wrappers grouping fewer than one composition, and absorb a grp at
/// the head of its enclosing group.
fn elim_grps(doc: DenullDoc) -> DenullDoc {
    // Backward pass: whether each node is in head position of its enclosing
    // group. Roots are; a comp's left operand inherits, its right does not;
    // seq resets.
    let mut head = vec![false; doc.objs.len()];
    for row in &doc.rows {
        match row {
            DenullRow::Break(id) | DenullRow::Line(id) => head[*id as usize] = true,
            DenullRow::Empty => {}
        }
    }
    for i in (0..doc.objs.len()).rev() {
        match doc.objs[i] {
            DenullObj::Grp(c) => head[c as usize] = head[i],
            DenullObj::Seq(c) => head[c as usize] = false,
            DenullObj::Comp(l, r, _) => {
                head[l as usize] = head[i];
                head[r as usize] = false;
            }
            DenullObj::Term(_) | DenullObj::Fix(_) => {}
        }
    }
    // Forward pass: rebuild bottom-up. A grp contributes no composition to its
    // enclosing count, so the count resets to Zero wherever a wrapper is kept
    // or dropped for grouping too little; an absorbed head grp passes its
    // count through.
    let mut objs: Vec<DenullObj> = Vec::with_capacity(doc.objs.len());
    let mut out: Vec<DObjId> = Vec::with_capacity(doc.objs.len());
    let mut count: Vec<Count> = Vec::with_capacity(doc.objs.len());
    for (i, node) in doc.objs.into_iter().enumerate() {
        let (c, id) = match node {
            DenullObj::Term(_) | DenullObj::Fix(_) => {
                (Count::Zero, leaf_obj(&mut objs, &doc.fixes, node))
            }
            DenullObj::Seq(c1) => (
                count[c1 as usize],
                push(&mut objs, DenullObj::Seq(out[c1 as usize])),
            ),
            DenullObj::Grp(c1) => {
                let (cc, cid) = (count[c1 as usize], out[c1 as usize]);
                if head[i] {
                    // At the head of the enclosing group: absorb the wrapper.
                    (cc, cid)
                } else {
                    match cc {
                        Count::Zero => (Count::Zero, cid),
                        Count::One | Count::Many => {
                            (Count::Zero, push(&mut objs, DenullObj::Grp(cid)))
                        }
                    }
                }
            }
            DenullObj::Comp(l, r, pad) => (
                add(Count::One, add(count[l as usize], count[r as usize])),
                push(
                    &mut objs,
                    DenullObj::Comp(out[l as usize], out[r as usize], pad),
                ),
            ),
        };
        count.push(c);
        out.push(id);
    }
    DenullDoc {
        rows: doc.rows.into_iter().map(|r| map_row(r, &out)).collect(),
        objs,
        fixes: doc.fixes,
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
    let mut objs: Vec<DenullObj> = Vec::with_capacity(n);
    // Chain state per input node: `head`/`tail` are input ids of the chain's
    // endpoints, `atom_out` is an atom's output id, and `next[tail] = (pad,
    // head-of-next)` links adjacent atoms.
    let mut atom_out: Vec<DObjId> = vec![0; n];
    let mut head: Vec<DObjId> = vec![0; n];
    let mut tail: Vec<DObjId> = vec![0; n];
    let mut next: Vec<Option<(bool, DObjId)>> = vec![None; n];

    fn materialize<'a>(
        objs: &mut Vec<DenullObj<'a>>,
        atom_out: &[DObjId],
        next: &[Option<(bool, DObjId)>],
        start: DObjId,
    ) -> DObjId {
        // Collect the chain's atoms (output ids) and the pads between them.
        let mut atoms: Vec<DObjId> = Vec::new();
        let mut pads: Vec<bool> = Vec::new();
        let mut cur = start;
        loop {
            atoms.push(atom_out[cur as usize]);
            match next[cur as usize] {
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
            result = push(objs, DenullObj::Comp(atoms[k], result, pads[k]));
        }
        result
    }

    for (i, node) in doc.objs.into_iter().enumerate() {
        match node {
            DenullObj::Term(_) | DenullObj::Fix(_) => {
                atom_out[i] = push(&mut objs, node);
                head[i] = i as DObjId;
                tail[i] = i as DObjId;
            }
            DenullObj::Grp(c) => {
                let spine = materialize(&mut objs, &atom_out, &next, head[c as usize]);
                atom_out[i] = push(&mut objs, DenullObj::Grp(spine));
                head[i] = i as DObjId;
                tail[i] = i as DObjId;
            }
            DenullObj::Seq(c) => {
                let spine = materialize(&mut objs, &atom_out, &next, head[c as usize]);
                atom_out[i] = push(&mut objs, DenullObj::Seq(spine));
                head[i] = i as DObjId;
                tail[i] = i as DObjId;
            }
            DenullObj::Comp(l, r, pad) => {
                next[tail[l as usize] as usize] = Some((pad, head[r as usize]));
                head[i] = head[l as usize];
                tail[i] = tail[r as usize];
            }
        }
    }

    let rows = doc
        .rows
        .into_iter()
        .map(|row| match row {
            DenullRow::Empty => DenullRow::Empty,
            DenullRow::Break(id) => {
                DenullRow::Break(materialize(&mut objs, &atom_out, &next, head[id as usize]))
            }
            DenullRow::Line(id) => {
                DenullRow::Line(materialize(&mut objs, &atom_out, &next, head[id as usize]))
            }
        })
        .collect();

    DenullDoc {
        rows,
        objs,
        fixes: doc.fixes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::DenullTerm;

    /// Far past where a native-stack recursion could survive; with flat arenas
    /// the folds are plain loops, so this now guards sizing behavior only.
    const DEEP: usize = 50_000;

    fn term(text: &'static str) -> DenullObj<'static> {
        DenullObj::Term(DenullTerm {
            props: Vec::new(),
            text,
        })
    }

    fn line_doc(objs: Vec<DenullObj>, root: DObjId) -> DenullDoc {
        DenullDoc {
            rows: vec![DenullRow::Line(root)],
            objs,
            fixes: Vec::new(),
        }
    }

    #[test]
    fn normalize_right_associates_deep_left_nested_comp() {
        // Left-nested comp chain, no grp/seq: normalization rebuilds it
        // right-nested (a right spine of DEEP compositions).
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut cur = push(&mut objs, term("a"));
        for _ in 0..DEEP {
            let right = push(&mut objs, term("b"));
            cur = push(&mut objs, DenullObj::Comp(cur, right, false));
        }
        let out = normalize(line_doc(objs, cur));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        let mut count = 0usize;
        let mut walk = root;
        while let DenullObj::Comp(_left, right, _pad) = out.objs[walk as usize] {
            count += 1;
            walk = right;
        }
        assert!(matches!(out.objs[walk as usize], DenullObj::Term(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn normalize_collapses_deep_seq_nesting() {
        // Deep seq nesting over a single term collapses away entirely.
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut cur = push(&mut objs, term("x"));
        for _ in 0..DEEP {
            cur = push(&mut objs, DenullObj::Seq(cur));
        }
        let out = normalize(line_doc(objs, cur));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        assert!(matches!(out.objs[root as usize], DenullObj::Term(_)));
    }

    #[test]
    fn normalize_collapses_deep_grp_nesting() {
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut cur = push(&mut objs, term("x"));
        for _ in 0..DEEP {
            cur = push(&mut objs, DenullObj::Grp(cur));
        }
        let out = normalize(line_doc(objs, cur));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        assert!(matches!(out.objs[root as usize], DenullObj::Term(_)));
    }

    #[test]
    fn normalize_handles_long_doc_spine() {
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut rows: Vec<DenullRow> = Vec::new();
        for _ in 0..DEEP {
            rows.push(DenullRow::Break(push(&mut objs, term("x"))));
        }
        let doc = DenullDoc {
            rows,
            objs,
            fixes: Vec::new(),
        };
        let out = normalize(doc);
        assert_eq!(out.rows.len(), DEEP);
    }

    #[test]
    fn seq_of_one_comp_is_dropped_but_two_kept() {
        // A seq grouping a single composition collapses; a seq grouping two or
        // more is kept.
        let mut objs: Vec<DenullObj> = Vec::new();
        let a = push(&mut objs, term("a"));
        let b = push(&mut objs, term("b"));
        let comp = push(&mut objs, DenullObj::Comp(a, b, false));
        let one = push(&mut objs, DenullObj::Seq(comp));
        let out = elim_seqs(line_doc(objs, one));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        assert!(
            matches!(out.objs[root as usize], DenullObj::Comp(..)),
            "seq of one comp dropped"
        );

        let mut objs: Vec<DenullObj> = Vec::new();
        let a = push(&mut objs, term("a"));
        let b = push(&mut objs, term("b"));
        let ab = push(&mut objs, DenullObj::Comp(a, b, false));
        let c = push(&mut objs, term("c"));
        let abc = push(&mut objs, DenullObj::Comp(ab, c, false));
        let two = push(&mut objs, DenullObj::Seq(abc));
        let out = elim_seqs(line_doc(objs, two));
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a line");
        };
        assert!(
            matches!(out.objs[root as usize], DenullObj::Seq(_)),
            "seq of two comps kept"
        );
    }
}
