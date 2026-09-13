//! lower: RebuildDoc → Doc
//!
//! The last pass applies three rule sets that the reference implementation
//! runs as five separate tree rewrites, in this order (the rules are not
//! confluent, so the order is part of the semantics):
//!
//! 1. **Denull.** Empty texts vanish, wrappers and all. A composition whose
//!    left operand vanished forwards its pad to the composition that will
//!    sit to its left (`a + (null & b)` pads like `a + b`); a grp/seq or a
//!    line discards a forwarded pad. Runs drop their empty terms and merge
//!    the pads between survivors.
//! 2. **Identities.** A seq grouping fewer than two compositions is dropped,
//!    and a seq directly under a seq is absorbed. Then a grp grouping no
//!    composition is dropped, and a grp at the head of its enclosing group
//!    is absorbed. Compositions inside a nested grp do not count toward the
//!    seq rule; compositions inside any wrapper do not count toward the grp
//!    rule.
//! 3. **Reassociation and rescoping.** Composition trees are
//!    right-associated inside each surviving grp/seq, and the nest/pack
//!    prefix two operands share is factored out around their composition,
//!    with the leftovers applied to each operand.
//!
//! The input is a flat postorder arena, so this is three loops with side
//! tables rather than five arena rebuilds: a forward loop for survival, pad
//! forwarding and the seq count; a backward loop for the inherited context
//! (under a seq, at the head of a group) that decides which seqs survive; and
//! a forward loop that decides grps, threads each composition tree as a
//! chain of atoms, and materializes every chain right-nested with its
//! prefixes factored, straight into the [`DocBuilder`].

use crate::arena::{Arena, IdVec, Range};
use crate::doc::{Doc, DocBuilder, ObjId, ObjNode};
use crate::layout::Pad;
use crate::resolve_scopes::{Obj, ObjId as RId, RebuildDoc};
use crate::serialize::{FixedDoc, PathId, PathNode, Prop, Run};

/// How many compositions a subtree contributes to its enclosing wrapper,
/// saturating at `Many`.
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

/// A lowered atom: the nest/pack props still to be applied around it
/// (outermost first, a range into the shared prop buffer) and its object.
type Atom = (Range<Prop>, ObjId);

/// The shared prop buffer terms' wrapper paths are materialized into,
/// memoized per path id: sibling terms under the same wrappers share one
/// materialization, so the buffer is O(path arena), not O(terms × depth).
struct Props {
    buf: Vec<Prop>,
    memo: IdVec<PathNode, Option<Range<Prop>>>,
}

impl Props {
    /// The props of `path`, outermost first.
    fn of(&mut self, paths: &Arena<PathNode>, path: Option<PathId>) -> Range<Prop> {
        let Some(path) = path else {
            return Range::EMPTY;
        };
        if let Some(range) = self.memo[path] {
            return range;
        }
        let start = self.buf.len();
        let mut cur = Some(path);
        while let Some(id) = cur {
            self.buf.push(paths[id].prop);
            cur = paths[id].parent;
        }
        // The path walk yields innermost-first; prop lists are outermost-first.
        self.buf[start..].reverse();
        let range = Range::new(start, self.buf.len());
        self.memo[path] = Some(range);
        range
    }
}

pub(crate) fn lower<'a>(doc: &RebuildDoc<'a>, fixed: &FixedDoc<'a>) -> Doc {
    let n = doc.objs.len();
    let mut b = DocBuilder::with_capacity(n);
    let mut props = Props {
        buf: Vec::new(),
        memo: IdVec::filled(None, fixed.paths.len()),
    };

    // Loop 1, forward: survival, the pad a node forwards to its left (the
    // identity `Unpadded` when none), the seq count, and the lowered runs.
    let mut alive: IdVec<Obj, bool> = IdVec::with_capacity(n);
    let mut fwd: IdVec<Obj, Pad> = IdVec::with_capacity(n);
    let mut seq_count: IdVec<Obj, Count> = IdVec::with_capacity(n);
    let mut atom: IdVec<Obj, Option<Atom>> = IdVec::filled(None, n);
    for (i, node) in doc.objs.iter() {
        let (live, pad, count) = match *node {
            Obj::Run(run) => match lower_run(&mut b, &mut props, fixed, run) {
                None => (false, Pad::Unpadded, Count::Zero),
                Some(a) => {
                    atom[i] = Some(a);
                    (true, Pad::Unpadded, Count::Zero)
                }
            },
            // A grp is opaque to the seq count.
            Obj::Grp(c) => (alive[c], Pad::Unpadded, Count::Zero),
            Obj::Seq(c) => (alive[c], Pad::Unpadded, seq_count[c]),
            Obj::Comp(l, r, pad) => match (alive[l], alive[r]) {
                (false, false) => (false, Pad::Unpadded, Count::Zero),
                (false, true) => (true, pad.merge(fwd[r]), seq_count[r]),
                (true, false) => (true, Pad::Unpadded, seq_count[l]),
                (true, true) => (
                    true,
                    Pad::Unpadded,
                    add(Count::One, add(seq_count[l], seq_count[r])),
                ),
            },
        };
        alive.push(live);
        fwd.push(pad);
        seq_count.push(count);
    }

    // Loop 2, backward: inherited context. `under` is "directly under a seq"
    // (a grp resets it); `head` is "at the head of the enclosing group" (a
    // surviving seq resets it, a composition's right operand is never at the
    // head). A vanished operand makes its sibling stand in the composition's
    // place, and a dropped or absorbed seq is transparent.
    let mut under: IdVec<Obj, bool> = IdVec::filled(false, n);
    let mut head: IdVec<Obj, bool> = IdVec::filled(false, n);
    let mut kept: IdVec<Obj, bool> = IdVec::filled(false, n);
    for root in doc.lines.iter() {
        head[*root] = true;
    }
    for (i, node) in doc.objs.iter().rev() {
        if !alive[i] {
            continue;
        }
        match *node {
            Obj::Run(_) => {}
            Obj::Grp(c) => {
                under[c] = false;
                head[c] = head[i];
            }
            Obj::Seq(c) => {
                let keep = !under[i] && matches!(seq_count[c], Count::Many);
                kept[i] = keep;
                under[c] = true;
                head[c] = !keep && head[i];
            }
            Obj::Comp(l, r, _) => match (alive[l], alive[r]) {
                (true, true) => {
                    under[l] = under[i];
                    under[r] = under[i];
                    head[l] = head[i];
                    head[r] = false;
                }
                (true, false) => {
                    under[l] = under[i];
                    head[l] = head[i];
                }
                (false, true) => {
                    under[r] = under[i];
                    head[r] = head[i];
                }
                (false, false) => unreachable!("a live composition has a live operand"),
            },
        }
    }

    // Loop 3, forward: the grp count and grp survival, then each composition
    // tree threaded as a chain of atoms — `next[tail] = (pad, head of the
    // next atom)` — materialized at every surviving wrapper and line root.
    let mut grp_count: IdVec<Obj, Count> = IdVec::with_capacity(n);
    let mut chain_head: IdVec<Obj, RId> = IdVec::with_capacity(n);
    let mut chain_tail: IdVec<Obj, RId> = IdVec::with_capacity(n);
    let mut next: IdVec<Obj, Option<(Pad, RId)>> = IdVec::filled(None, n);
    let mut scratch = Chain {
        atoms: Vec::new(),
        pads: Vec::new(),
    };
    for (i, node) in doc.objs.iter() {
        let (count, first, last) = match *node {
            _ if !alive[i] => (Count::Zero, i, i),
            Obj::Run(_) => (Count::Zero, i, i),
            Obj::Seq(c) if kept[i] => {
                let (p, obj) = scratch.materialize(&mut b, &props, &atom, &next, chain_head[c]);
                atom[i] = Some((p, b.obj(ObjNode::Seq(obj))));
                (grp_count[c], i, i)
            }
            Obj::Seq(c) => (grp_count[c], chain_head[c], chain_tail[c]),
            // At the head of its group a grp is absorbed and its count
            // passes through; elsewhere it is dropped when it groups nothing
            // and otherwise kept, contributing no composition either way.
            Obj::Grp(c) if head[i] => (grp_count[c], chain_head[c], chain_tail[c]),
            Obj::Grp(c) if matches!(grp_count[c], Count::Zero) => {
                (Count::Zero, chain_head[c], chain_tail[c])
            }
            Obj::Grp(c) => {
                let (p, obj) = scratch.materialize(&mut b, &props, &atom, &next, chain_head[c]);
                atom[i] = Some((p, b.obj(ObjNode::Grp(obj))));
                (Count::Zero, i, i)
            }
            Obj::Comp(l, r, pad) => match (alive[l], alive[r]) {
                (true, true) => {
                    next[chain_tail[l]] = Some((pad.merge(fwd[r]), chain_head[r]));
                    (
                        add(Count::One, add(grp_count[l], grp_count[r])),
                        chain_head[l],
                        chain_tail[r],
                    )
                }
                (true, false) => (grp_count[l], chain_head[l], chain_tail[l]),
                (false, true) => (grp_count[r], chain_head[r], chain_tail[r]),
                (false, false) => unreachable!("a live composition has a live operand"),
            },
        };
        grp_count.push(count);
        chain_head.push(first);
        chain_tail.push(last);
    }

    let lines = doc
        .lines
        .iter()
        .map(|&root| {
            if !alive[root] {
                return None;
            }
            let (p, obj) = scratch.materialize(&mut b, &props, &atom, &next, chain_head[root]);
            Some(wrap_props(&mut b, p.slice(&props.buf), obj))
        })
        .collect();
    b.finish(lines)
}

/// Lowers a run: drops its empty terms, merges the pads between survivors
/// (a dropped term's padding on either side folds into the one composition
/// that remains), and keeps the first surviving term's props as the run's.
/// `None` if nothing survived.
fn lower_run<'a>(
    b: &mut DocBuilder,
    props: &mut Props,
    fixed: &FixedDoc<'a>,
    run: Run<'a>,
) -> Option<Atom> {
    let terms = run.terms.slice(&fixed.terms);
    let seps = run.seps.slice(&fixed.run_seps);
    let mut first_props: Option<Range<Prop>> = None;
    // The pad to put before the next survivor: `None` until the first
    // survivor (its leading pads are dropped), then the merge of every pad
    // since the previous survivor.
    let mut pending: Option<Pad> = None;
    let start = b.start_run();
    for (k, term) in terms.iter().enumerate() {
        if !term.text.is_empty() {
            b.push_text(pending.unwrap_or(Pad::Unpadded), term.text);
            if first_props.is_none() {
                first_props = Some(props.of(&fixed.paths, term.path));
            }
            pending = Some(Pad::Unpadded);
        }
        if let (Some(p), Some(sep)) = (pending.as_mut(), seps.get(k)) {
            *p = p.merge(sep.pad);
        }
    }
    let first_props = first_props?;
    Some((first_props, b.end_run(start)))
}

/// Scratch for materializing one chain, reused across chains.
struct Chain {
    atoms: Vec<Atom>,
    pads: Vec<Pad>,
}

impl Chain {
    /// Materializes the chain starting at `start` as a right-nested
    /// composition spine, factoring at each composition the nest/pack prefix
    /// its operands share. Returns the spine's remaining props and object.
    fn materialize(
        &mut self,
        b: &mut DocBuilder,
        props: &Props,
        atom: &IdVec<Obj, Option<Atom>>,
        next: &IdVec<Obj, Option<(Pad, RId)>>,
        start: RId,
    ) -> Atom {
        self.atoms.clear();
        self.pads.clear();
        let mut cur = start;
        loop {
            self.atoms
                .push(atom[cur].expect("every chain element is an atom"));
            match next[cur] {
                Some((pad, id)) => {
                    self.pads.push(pad);
                    cur = id;
                }
                None => break,
            }
        }
        // Rebuild right-nested: Comp(a0, Comp(a1, ..., p1), p0), innermost
        // first so each composition sees its right operand's leftover props.
        let (mut res_props, mut result) = *self.atoms.last().expect("a chain has an atom");
        for k in (0..self.pads.len()).rev() {
            let (l_props, left) = self.atoms[k];
            let l = l_props.slice(&props.buf);
            let r = res_props.slice(&props.buf);
            let common = l.iter().zip(r.iter()).take_while(|(a, b)| a == b).count();
            let left = wrap_props(b, &l[common..], left);
            let right = wrap_props(b, &r[common..], result);
            result = b.obj(ObjNode::Comp(left, right, self.pads[k]));
            res_props = Range::new(l_props.start(), l_props.start() + common);
        }
        (res_props, result)
    }
}

/// Wraps an object with its props (index 0 outermost), returning the id of the
/// outermost wrapper.
fn wrap_props(b: &mut DocBuilder, props: &[Prop], obj: ObjId) -> ObjId {
    // Apply from the tail so the first prop ends up outermost.
    let mut obj = obj;
    for prop in props.iter().rev() {
        obj = match prop {
            Prop::Nest => b.obj(ObjNode::Nest(obj)),
            Prop::Pack(index) => b.obj(ObjNode::Pack(*index, obj)),
        };
    }
    obj
}
