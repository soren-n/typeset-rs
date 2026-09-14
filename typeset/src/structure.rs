//! structure: `FixedDoc` → Doc (solve the grp/seq scopes per line, then emit)
//!
//! Scopes are ranges over a line's items, and items only exist once
//! `serialize` has coalesced fixed compositions into runs, so a scope's
//! extent is not readable off the layout tree. Per line, every item is a
//! node and every scope an edge from the node it opened at to the node it
//! closed at: the graph is the direct representation of those ranges, and
//! the reference implementation defines the widening rules over it.
//!
//! 1. [`Graph::build`] replays each composition's scope deltas (the stack
//!    pushes and pops `serialize` recorded) into edges, in the order the
//!    scopes opened, which is document pre-order — the order `solve` and
//!    the emitter depend on.
//! 2. [`Graph::solve`] resolves nodes that both close and open scopes (a
//!    run that straddles a scope boundary, as in `grp(a + b) !& c`) by
//!    widening: leading seq edges out of the node are re-sourced onto the
//!    incoming side, and the incoming edges are handed forward past the
//!    first grp edge out.
//! 3. [`Emitter`] reads each line back into the [`Doc`], applying on the
//!    way the rules the reference runs as five tree rewrites afterwards:
//!    empty terms vanish, trivial grp/seq wrappers are dropped, every spine
//!    is right-nested, and shared nest/pack prefixes are factored out.
//!
//! The graph is a side table over the `FixedDoc`'s item buffer (a node *is*
//! its item's id) plus one edge arena. A node's incident edges are intrusive
//! linked lists threaded through the edge arena, so `solve`'s surgery (pop a
//! list head, insert before a known edge, splice one list into another) is
//! O(1) pointer rewiring and building the graph allocates nothing per node
//! or edge.

use crate::arena::{Arena, Id, IdVec, Range};
use crate::doc::{Doc, DocBuilder, ObjId, ObjNode};
use crate::layout::Pad;
use crate::serialize::{FixedComp, FixedDoc, FixedLine, PathId, PathNode, Prop, Run, ScopeKind};

pub(crate) fn structure(doc: &FixedDoc) -> Doc {
    let mut graph = Graph::build(doc);
    graph.solve();
    Emitter::new(doc, &graph).emit()
}

// --- The scope graph -------------------------------------------------------

/// A node is its item: nodes are the items of the `FixedDoc`, by id.
type NodeId<'a> = Id<Run<'a>>;
type EdgeId<'a> = Id<Edge<'a>>;

/// A node's ends of its intrusive edge lists.
#[derive(Debug, Copy, Clone)]
struct Node<'a> {
    /// Edges targeting this node, in list order (solve depends on the order).
    ins_head: Option<EdgeId<'a>>,
    ins_tail: Option<EdgeId<'a>>,
    /// Number of edges on the ins list.
    ins_len: u32,
    /// Edges sourced at this node, in list order (solve and the emitter
    /// depend on the order).
    outs_head: Option<EdgeId<'a>>,
    outs_tail: Option<EdgeId<'a>>,
}

impl Node<'_> {
    const EMPTY: Self = Node {
        ins_head: None,
        ins_tail: None,
        ins_len: 0,
        outs_head: None,
        outs_tail: None,
    };
}

#[derive(Debug)]
struct Edge<'a> {
    kind: ScopeKind,
    source: NodeId<'a>,
    target: NodeId<'a>,
    /// Links within the source's outs list. `prev_out` exists because solve
    /// inserts before an arbitrary known edge of that list.
    next_out: Option<EdgeId<'a>>,
    prev_out: Option<EdgeId<'a>>,
    /// Link within the target's ins list. Ins lists are only appended to,
    /// iterated forward, spliced in *after* a known edge, or taken whole, so
    /// they need no back link.
    next_in: Option<EdgeId<'a>>,
}

struct Graph<'a> {
    /// One node per item of the `FixedDoc`, index-aligned with its items.
    nodes: IdVec<Run<'a>, Node<'a>>,
    /// The shared edge arena the intrusive lists thread through.
    edges: Arena<Edge<'a>>,
}

/// A scope while its line is being built: where it opened, and where it
/// closed once it has.
#[derive(Copy, Clone)]
struct Slot<'a> {
    kind: ScopeKind,
    from: NodeId<'a>,
    to: Option<NodeId<'a>>,
}

impl<'a> Graph<'a> {
    /// Assigns a node per item and replays each composition's scope deltas
    /// at that node. A run's internal comps and its trailing separator all
    /// share the run's node, exactly as document order threads them.
    ///
    /// The open scopes form a stack (a composition's closes pop from it, its
    /// opens push onto it), and scopes open in document pre-order, so
    /// materializing edges in opening order gives every node's ins and outs
    /// lists in pre-order.
    fn build(doc: &FixedDoc<'a>) -> Graph<'a> {
        let mut g = Graph {
            nodes: IdVec::filled(Node::EMPTY, doc.items.len()),
            edges: Arena::new(),
        };
        // Per-line scratch, reused across lines: the scopes of the line in
        // opening order, and the indices of those still open.
        let mut slots: Vec<Slot<'a>> = Vec::new();
        let mut open: Vec<usize> = Vec::new();
        for line in &doc.lines {
            slots.clear();
            open.clear();
            let mut apply = |node: NodeId<'a>, comp: &FixedComp| {
                for _ in 0..comp.closes {
                    let slot = open.pop().expect("a closed scope was opened");
                    slots[slot].to = Some(node);
                }
                for &kind in comp.opens.slice(&doc.scopes) {
                    open.push(slots.len());
                    slots.push(Slot {
                        kind,
                        from: node,
                        to: None,
                    });
                }
            };
            let items = line.items.slice(&doc.items);
            let seps = line.seps.slice(&doc.item_seps);
            for (i, run) in items.iter().enumerate() {
                let node = line.items.id_at(i);
                for sep in run.seps.slice(&doc.run_seps) {
                    apply(node, sep);
                }
                if let Some(sep) = seps.get(i) {
                    apply(node, sep);
                }
            }
            // Every scope still open closes at the line's last node.
            let last = line.items.id_at(items.len() - 1);
            for &slot in &open {
                slots[slot].to = Some(last);
            }
            for slot in &slots {
                let to = slot.to.expect("every scope closes by the end of its line");
                if slot.from != to {
                    g.add_edge(slot.kind, slot.from, to);
                }
            }
        }
        g
    }

    /// Appends an edge to its source's outs list and its target's ins list.
    fn add_edge(&mut self, kind: ScopeKind, source: NodeId<'a>, target: NodeId<'a>) {
        let edge = self.edges.push(Edge {
            kind,
            source,
            target,
            next_out: None,
            prev_out: None,
            next_in: None,
        });
        let src = &mut self.nodes[source];
        self.edges[edge].prev_out = src.outs_tail;
        match src.outs_tail {
            None => src.outs_head = Some(edge),
            Some(tail) => self.edges[tail].next_out = Some(edge),
        }
        self.nodes[source].outs_tail = Some(edge);
        let tgt = &mut self.nodes[target];
        match tgt.ins_tail {
            None => tgt.ins_head = Some(edge),
            Some(tail) => self.edges[tail].next_in = Some(edge),
        }
        self.nodes[target].ins_tail = Some(edge);
        self.nodes[target].ins_len += 1;
    }

    /// Resolves every node with both incoming and outgoing edges. Edges never
    /// cross lines, so one pass over the node table covers the document.
    fn solve(&mut self) {
        for node in self.nodes.ids() {
            self.solve_node(node);
        }
    }

    fn solve_node(&mut self, node: NodeId<'a>) {
        let (Some(ins_head), Some(_)) = (self.nodes[node].ins_head, self.nodes[node].outs_head)
        else {
            return;
        };

        // The incoming edge whose source is leftmost (first on ties: forward
        // iteration only replaces on strictly smaller sources).
        let mut ins_first = ins_head;
        let mut best_src = self.edges[ins_head].source;
        let mut e = self.edges[ins_head].next_in;
        while let Some(edge) = e {
            let src = self.edges[edge].source;
            if src < best_src {
                best_src = src;
                ins_first = edge;
            }
            e = self.edges[edge].next_in;
        }

        // Walk this node's outgoing edges, moving each leading seq edge out of
        // the way, until the first grp edge (or the end). A moved seq edge is
        // re-sourced onto the working edge's source, inserted immediately
        // before it, and becomes the working edge itself — so successive seq
        // edges stack up in front of `ins_first`.
        let mut edge = ins_first;
        let grp = loop {
            let Some(curr) = self.nodes[node].outs_head else {
                break None;
            };
            match self.edges[curr].kind {
                ScopeKind::Grp => break Some(curr),
                ScopeKind::Seq => {
                    self.pop_out_head(node);
                    let src = self.edges[edge].source;
                    self.insert_out_before(src, curr, edge);
                    self.edges[curr].source = src;
                    edge = curr;
                }
            }
        };

        // Hand this node's whole incoming list forward past the grp edge:
        // retarget every incoming edge to the grp's target and splice the list
        // immediately after the grp edge in that target's ins list.
        if let Some(grp) = grp {
            let head = self.nodes[node].ins_head;
            let tail = self.nodes[node].ins_tail.expect("ins list has a tail");
            let len = self.nodes[node].ins_len;
            self.nodes[node].ins_head = None;
            self.nodes[node].ins_tail = None;
            self.nodes[node].ins_len = 0;

            let target = self.edges[grp].target;
            let mut e = head;
            while let Some(edge) = e {
                self.edges[edge].target = target;
                e = self.edges[edge].next_in;
            }

            let after = self.edges[grp].next_in;
            self.edges[grp].next_in = head;
            self.edges[tail].next_in = after;
            if after.is_none() {
                self.nodes[target].ins_tail = Some(tail);
            }
            self.nodes[target].ins_len += len;
        }
    }

    /// Detaches the head edge of `node`'s outs list.
    fn pop_out_head(&mut self, node: NodeId<'a>) {
        let head = self.nodes[node].outs_head.expect("outs list is non-empty");
        let next = self.edges[head].next_out;
        self.nodes[node].outs_head = next;
        match next {
            None => self.nodes[node].outs_tail = None,
            Some(next) => self.edges[next].prev_out = None,
        }
        self.edges[head].next_out = None;
    }

    /// Inserts `new` immediately before `before` in `src`'s outs list.
    fn insert_out_before(&mut self, src: NodeId<'a>, new: EdgeId<'a>, before: EdgeId<'a>) {
        let prev = self.edges[before].prev_out;
        self.edges[new].prev_out = prev;
        self.edges[new].next_out = Some(before);
        self.edges[before].prev_out = Some(new);
        match prev {
            None => self.nodes[src].outs_head = Some(new),
            Some(prev) => self.edges[prev].next_out = Some(new),
        }
    }
}

// --- Emitting the document -------------------------------------------------
//
// After `solve`, a line's scopes nest: every node either closes scopes or
// opens them, never both, and the scopes open at any item form a stack. So
// a line reads back as a tree of *spines* — the line's own, and one per
// scope — each a left-to-right sequence of elements (an item, or a nested
// scope) with a pad between neighbours. The emitter walks each line twice
// with a stack of open spines:
//
// - A counting walk decides which seqs survive. Empty items vanish; a scope
//   with no surviving element vanishes with them. A seq is kept when it
//   groups two or more compositions (a grp beneath it is opaque, a seq is
//   transparent) and is not directly under a seq.
// - The emitting walk lowers each run, threads the pad between surviving
//   neighbours (a vanished element's pads merge into the one composition
//   that remains; a spine's leading and trailing pads are dropped), decides
//   the grps (dropped when they group no composition, absorbed at the head
//   of their enclosing spine; seqs are transparent to the count), composes
//   each surviving spine right-nested with the nest/pack prefix its
//   operands share factored out, and splices a dropped scope's elements
//   into the spine that encloses it.
//
// The reference runs these as separate tree rewrites in exactly this order
// (null removal, seq identities, grp identities, reassociation, rescoping);
// the rules are not confluent, so the order is part of the semantics, and
// the two walks reproduce it: seq survival never depends on a grp
// decision, while grp absorption depends on which seqs survive.

/// How many compositions a spine contributes to its enclosing scope,
/// saturating at `Many`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Count {
    Zero,
    One,
    Many,
}

impl Count {
    fn add(self, other: Count) -> Count {
        match (self, other) {
            (Count::Zero, _) => other,
            (_, Count::Zero) => self,
            (Count::Many, _) | (_, Count::Many) | (Count::One, Count::One) => Count::Many,
        }
    }

    /// The compositions between `elements` neighbours.
    fn between(elements: usize) -> Count {
        match elements {
            0 | 1 => Count::Zero,
            2 => Count::One,
            _ => Count::Many,
        }
    }
}

/// A lowered element: the nest/pack props still to be applied around it
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

/// A spine on the counting walk's stack.
struct Counting<'a> {
    /// The scope's edge; `None` for the line's own spine.
    edge: Option<EdgeId<'a>>,
    seq: bool,
    /// Surviving elements so far.
    elements: usize,
    /// Compositions contributed by transparent (seq) children.
    inner: Count,
}

/// What becomes of a spine when it closes.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Wrap {
    /// The line's own spine: its root object, if any.
    Line,
    /// A grp, unless it turns out to group no composition.
    Grp,
    /// A surviving seq.
    Seq,
    /// A dropped or absorbed scope: its elements join the enclosing spine.
    Splice,
}

/// A spine on the emitting walk's stack.
struct Spine {
    wrap: Wrap,
    /// Where this spine's elements begin in the shared element buffer.
    start: usize,
    /// Whether the spine's first surviving element is at the head of its
    /// enclosing group, which absorbs a grp there.
    head: bool,
    /// The pad to put before the next surviving element: `None` until the
    /// first survivor (leading pads are dropped), then the merge of every
    /// pad since the previous survivor.
    pending: Option<Pad>,
    /// Compositions contributed by surviving seq children, which are
    /// transparent to the grp count.
    inner: Count,
}

struct Emitter<'d, 'a> {
    fixed: &'d FixedDoc<'a>,
    g: &'d Graph<'a>,
    b: DocBuilder,
    props: Props,
    /// Which seq edges survive, from the counting walk.
    kept: IdVec<Edge<'a>, bool>,
    // Scratch, reused across lines.
    counting: Vec<Counting<'a>>,
    spines: Vec<Spine>,
    /// The elements of every open spine, each with the pad before it (the
    /// first element of a spine ignores its pad).
    elements: Vec<(Pad, Atom)>,
}

/// Whether a run has any non-empty term.
fn alive<'a>(fixed: &FixedDoc<'a>, run: Run<'a>) -> bool {
    run.terms
        .slice(&fixed.terms)
        .iter()
        .any(|term| !term.text.is_empty())
}

impl<'d, 'a> Emitter<'d, 'a> {
    fn new(fixed: &'d FixedDoc<'a>, g: &'d Graph<'a>) -> Self {
        Emitter {
            fixed,
            g,
            // Every surviving item yields at least one object, so the item
            // total is a capacity floor for the object arena.
            b: DocBuilder::with_capacity(fixed.items.len()),
            props: Props {
                buf: Vec::new(),
                memo: IdVec::filled(None, fixed.paths.len()),
            },
            kept: IdVec::filled(false, g.edges.len()),
            counting: Vec::new(),
            spines: Vec::new(),
            elements: Vec::new(),
        }
    }

    fn emit(mut self) -> Doc {
        let lines = self
            .fixed
            .lines
            .iter()
            .map(|line| {
                self.count_line(line);
                self.emit_line(line)
            })
            .collect();
        self.b.finish(lines)
    }

    /// The counting walk: decides `kept` for every seq of the line.
    fn count_line(&mut self, line: &FixedLine<'a>) {
        let (fixed, g) = (self.fixed, self.g);
        let items = line.items.slice(&fixed.items);
        self.counting.clear();
        self.counting.push(Counting {
            edge: None,
            seq: false,
            elements: 0,
            inner: Count::Zero,
        });
        for (i, run) in items.iter().enumerate() {
            let node = &g.nodes[line.items.id_at(i)];
            // The item is the first element of every scope opening here.
            let mut e = node.outs_head;
            while let Some(edge) = e {
                self.counting.push(Counting {
                    edge: Some(edge),
                    seq: g.edges[edge].kind == ScopeKind::Seq,
                    elements: 0,
                    inner: Count::Zero,
                });
                e = g.edges[edge].next_out;
            }
            // ... and the last element of every scope closing here,
            // innermost first; each closed scope is then an element of the
            // next.
            let mut survives = alive(fixed, *run);
            for _ in 0..node.ins_len {
                let mut top = self.counting.pop().expect("a closing scope is open");
                top.elements += usize::from(survives);
                let count = Count::between(top.elements).add(top.inner);
                let parent = self.counting.last_mut().expect("the line is never popped");
                if top.seq {
                    let edge = top.edge.expect("only scopes close");
                    self.kept[edge] = !parent.seq && count == Count::Many;
                    parent.inner = parent.inner.add(count);
                }
                survives = top.elements > 0;
            }
            let top = self.counting.last_mut().expect("the line is never popped");
            top.elements += usize::from(survives);
        }
    }

    /// The emitting walk: the line's root object, `None` if nothing
    /// survives.
    fn emit_line(&mut self, line: &FixedLine<'a>) -> Option<ObjId> {
        let (fixed, g) = (self.fixed, self.g);
        let items = line.items.slice(&fixed.items);
        // `seps[i].pad` is the pad between item `i` and `i + 1`.
        let seps = line.seps.slice(&fixed.item_seps);
        self.spines.clear();
        self.elements.clear();
        self.spines.push(Spine {
            wrap: Wrap::Line,
            start: 0,
            head: true,
            pending: None,
            inner: Count::Zero,
        });
        for (i, run) in items.iter().enumerate() {
            let node = &g.nodes[line.items.id_at(i)];
            assert!(
                node.ins_len == 0 || node.outs_head.is_none(),
                "solve leaves no node both closing and opening scopes"
            );
            let mut e = node.outs_head;
            while let Some(edge) = e {
                self.open(edge);
                e = g.edges[edge].next_out;
            }
            let mut element = self.emit_run(*run);
            for _ in 0..node.ins_len {
                self.push(element);
                element = self.close();
            }
            self.push(element);
            match seps.get(i) {
                Some(sep) => {
                    let top = self.spines.last_mut().expect("the line is never popped");
                    top.pending = top.pending.map(|pad| pad.merge(sep.pad));
                }
                None => {
                    let top = self.spines.pop().expect("the line spine");
                    assert!(
                        self.spines.is_empty() && top.wrap == Wrap::Line,
                        "every scope closes by the end of its line"
                    );
                    return self.compose(top.start).map(|(props, obj)| {
                        wrap_props(&mut self.b, props.slice(&self.props.buf), obj)
                    });
                }
            }
        }
        unreachable!("every line has at least one item")
    }

    /// Opens the scope of `edge`: pushes its spine.
    fn open(&mut self, edge: EdgeId<'a>) {
        let parent = self.spines.last().expect("the line is never popped");
        // The scope is at the head of its enclosing group when it is the
        // first surviving element of a spine that is itself at the head.
        let head = parent.head && parent.pending.is_none();
        let (wrap, head) = match self.g.edges[edge].kind {
            ScopeKind::Grp if head => (Wrap::Splice, true),
            ScopeKind::Grp => (Wrap::Grp, false),
            ScopeKind::Seq if self.kept[edge] => (Wrap::Seq, false),
            ScopeKind::Seq => (Wrap::Splice, head),
        };
        self.spines.push(Spine {
            wrap,
            start: self.elements.len(),
            head,
            pending: None,
            inner: Count::Zero,
        });
    }

    /// Appends a surviving element to the innermost spine, after the pad
    /// pending since the previous survivor.
    fn push(&mut self, element: Option<Atom>) {
        let Some(atom) = element else {
            return;
        };
        let top = self.spines.last_mut().expect("the line is never popped");
        self.elements
            .push((top.pending.unwrap_or(Pad::Unpadded), atom));
        top.pending = Some(Pad::Unpadded);
    }

    /// Closes the innermost scope, returning it as one element of the
    /// enclosing spine — or nothing, when it vanished or its elements were
    /// spliced into the enclosing spine instead.
    fn close(&mut self) -> Option<Atom> {
        let top = self.spines.pop().expect("a closing scope is open");
        let parent = self.spines.last_mut().expect("the line is never popped");
        let count = Count::between(self.elements.len() - top.start).add(top.inner);
        let wrap = match top.wrap {
            Wrap::Grp if count == Count::Zero => Wrap::Splice,
            wrap => wrap,
        };
        match wrap {
            Wrap::Line => unreachable!("the line spine closes with the line"),
            Wrap::Splice => {
                if let Some(first) = self.elements.get_mut(top.start) {
                    first.0 = parent.pending.unwrap_or(Pad::Unpadded);
                    parent.pending = Some(Pad::Unpadded);
                }
                None
            }
            Wrap::Grp | Wrap::Seq => {
                if wrap == Wrap::Seq {
                    parent.inner = parent.inner.add(count);
                }
                let (props, obj) = self.compose(top.start)?;
                let obj = self.b.obj(match wrap {
                    Wrap::Grp => ObjNode::Grp(obj),
                    _ => ObjNode::Seq(obj),
                });
                Some((props, obj))
            }
        }
    }

    /// Composes the elements from `start` on as one right-nested spine,
    /// factoring at each composition the nest/pack prefix its operands
    /// share, and removes them. Returns the spine's remaining props and
    /// object, or nothing for an empty spine.
    fn compose(&mut self, start: usize) -> Option<Atom> {
        let elements = &self.elements[start..];
        let (mut res_props, mut result) = elements.last()?.1;
        // Innermost first, so each composition sees its right operand's
        // leftover props.
        for k in (1..elements.len()).rev() {
            let (l_props, left) = elements[k - 1].1;
            let pad = elements[k].0;
            let l = l_props.slice(&self.props.buf);
            let r = res_props.slice(&self.props.buf);
            let common = l.iter().zip(r.iter()).take_while(|(a, b)| a == b).count();
            let left = wrap_props(&mut self.b, &l[common..], left);
            let right = wrap_props(&mut self.b, &r[common..], result);
            result = self.b.obj(ObjNode::Comp(left, right, pad));
            res_props = Range::new(l_props.start(), l_props.start() + common);
        }
        self.elements.truncate(start);
        Some((res_props, result))
    }

    /// Lowers a run: drops its empty terms, merges the pads between
    /// survivors (a dropped term's padding on either side folds into the one
    /// composition that remains), and keeps the first surviving term's props
    /// as the run's. `None` if nothing survived.
    fn emit_run(&mut self, run: Run<'a>) -> Option<Atom> {
        let terms = run.terms.slice(&self.fixed.terms);
        let seps = run.seps.slice(&self.fixed.run_seps);
        let mut first_props: Option<Range<Prop>> = None;
        // The pad to put before the next survivor: `None` until the first
        // survivor (its leading pads are dropped), then the merge of every
        // pad since the previous survivor.
        let mut pending: Option<Pad> = None;
        let start = self.b.start_run();
        for (k, term) in terms.iter().enumerate() {
            if !term.text.is_empty() {
                self.b
                    .push_text(pending.unwrap_or(Pad::Unpadded), term.text);
                if first_props.is_none() {
                    first_props = Some(self.props.of(&self.fixed.paths, term.path));
                }
                pending = Some(Pad::Unpadded);
            }
            if let (Some(p), Some(sep)) = (pending.as_mut(), seps.get(k)) {
                *p = p.merge(sep.pad);
            }
        }
        let first_props = first_props?;
        Some((first_props, self.b.end_run(start)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructors::{comp, fix, grp, nest, null, pack, seq, text};
    use crate::layout::{Break, Layout};
    use crate::serialize::serialize;

    /// The document of a one-line layout, printed as nested constructor
    /// names over the runs' texts.
    fn shape(layout: &Layout) -> String {
        let fixed = serialize(&layout.nodes, &layout.text);
        let doc = structure(&fixed);
        let [root] = doc.lines[..] else {
            panic!("expected one line")
        };
        fn obj(doc: &Doc, id: ObjId) -> String {
            match doc.objs[id] {
                ObjNode::Run(range) => range.slice(&doc.text).to_string(),
                ObjNode::Grp(c) => format!("Grp({})", obj(doc, c)),
                ObjNode::Seq(c) => format!("Seq({})", obj(doc, c)),
                ObjNode::Nest(c) => format!("Nest({})", obj(doc, c)),
                ObjNode::Pack(_, c) => format!("Pack({})", obj(doc, c)),
                ObjNode::Comp(l, r, _) => format!("Comp({}, {})", obj(doc, l), obj(doc, r)),
            }
        }
        root.map_or_else(|| "Empty".to_string(), |root| obj(&doc, root))
    }

    fn pad(l: Layout, r: Layout) -> Layout {
        comp(l, r, Pad::Padded, Break::Breakable)
    }

    fn fixed(l: Layout, r: Layout) -> Layout {
        comp(l, r, Pad::Padded, Break::Fixed)
    }

    #[test]
    fn nested_scopes_are_rebuilt_as_wrappers() {
        let layout = pad(text("x"), pad(grp(pad(text("a"), text("b"))), text("c")));
        assert_eq!(shape(&layout), "Comp(x, Comp(Grp(Comp(a, b)), c))");
        let layout = seq(pad(
            text("a"),
            pad(text("b"), grp(pad(text("c"), text("d")))),
        ));
        assert_eq!(shape(&layout), "Seq(Comp(a, Comp(b, Grp(Comp(c, d)))))");
    }

    #[test]
    fn scopes_opening_together_nest_in_opening_order() {
        // x + grp(seq(a + b + c)): both scopes open at a and close at c; the
        // grp, opened first, wraps the seq.
        let layout = pad(
            text("x"),
            grp(seq(pad(text("a"), pad(text("b"), text("c"))))),
        );
        assert_eq!(shape(&layout), "Comp(x, Grp(Seq(Comp(a, Comp(b, c)))))");
    }

    #[test]
    fn scope_widens_to_cover_a_run_that_straddles_its_end() {
        // x + (grp(a + b) !+ c): the fixed composition coalesces b and c
        // into one run, so the grp cannot end between them; it widens to
        // include c.
        let layout = pad(text("x"), fixed(grp(pad(text("a"), text("b"))), text("c")));
        assert_eq!(shape(&layout), "Comp(x, Grp(Comp(a, b c)))");
    }

    #[test]
    fn straddled_scopes_resolve_seq_outward_and_grp_inward() {
        // seq(a + b + c) !+ grp(d + e): the run [c d] both closes the seq
        // and opens the grp. The seq's end is handed past the grp, so the
        // seq covers everything and the grp sits inside it.
        let layout = fixed(
            seq(pad(text("a"), pad(text("b"), text("c")))),
            grp(pad(text("d"), text("e"))),
        );
        assert_eq!(shape(&layout), "Seq(Comp(a, Comp(b, Grp(Comp(c d, e)))))");
    }

    #[test]
    fn a_fix_is_one_run() {
        let layout = fix(pad(pad(text("a"), text("b")), text("c")));
        assert_eq!(shape(&layout), "a b c");
    }

    #[test]
    fn spines_are_right_nested_across_dropped_scopes() {
        // (a + b) + grp(c) + d: the grp groups nothing and is dropped; the
        // whole line is one right-nested spine.
        let layout = pad(pad(text("a"), text("b")), pad(grp(text("c")), text("d")));
        assert_eq!(shape(&layout), "Comp(a, Comp(b, Comp(c, d)))");
    }

    #[test]
    fn a_grp_at_the_head_of_its_group_is_absorbed() {
        assert_eq!(
            shape(&pad(grp(pad(text("a"), text("b"))), text("c"))),
            "Comp(a, Comp(b, c))"
        );
        // Inside a kept seq the head resets, so the grp survives.
        assert_eq!(
            shape(&seq(pad(
                grp(pad(text("a"), text("b"))),
                pad(text("c"), text("d"))
            ))),
            "Seq(Comp(Grp(Comp(a, b)), Comp(c, d)))"
        );
    }

    #[test]
    fn a_seq_needs_two_compositions_and_no_seq_above_it() {
        assert_eq!(shape(&seq(pad(text("a"), text("b")))), "Comp(a, b)");
        assert_eq!(
            shape(&seq(pad(text("a"), seq(pad(text("b"), text("c")))))),
            "Seq(Comp(a, Comp(b, c)))"
        );
        // A grp beneath a seq is opaque to its count.
        assert_eq!(
            shape(&pad(
                text("x"),
                seq(grp(pad(text("a"), pad(text("b"), text("c")))))
            )),
            "Comp(x, Grp(Comp(a, Comp(b, c))))"
        );
    }

    #[test]
    fn empty_texts_vanish_and_their_pads_merge() {
        assert_eq!(shape(&null()), "Empty");
        assert_eq!(shape(&pad(nest(null()), text("a"))), "a");
        // The pads around a vanished middle element merge; a vanished
        // leading element's pad is dropped even across a wrapper.
        let layout = comp(
            text("a"),
            comp(null(), text("b"), Pad::Padded, Break::Breakable),
            Pad::Unpadded,
            Break::Breakable,
        );
        let fixed = serialize(&layout.nodes, &layout.text);
        let doc = structure(&fixed);
        let root = doc.lines[0].expect("a survives");
        assert!(matches!(doc.objs[root], ObjNode::Comp(_, _, Pad::Padded)));
        let layout = comp(
            text("a"),
            grp(comp(null(), text("b"), Pad::Padded, Break::Breakable)),
            Pad::Unpadded,
            Break::Breakable,
        );
        let fixed = serialize(&layout.nodes, &layout.text);
        let doc = structure(&fixed);
        let root = doc.lines[0].expect("a survives");
        assert!(matches!(doc.objs[root], ObjNode::Comp(_, _, Pad::Unpadded)));
    }

    #[test]
    fn shared_wrapper_prefixes_are_factored_out() {
        let layout = pack(nest(pad(text("a"), text("b"))));
        assert_eq!(shape(&layout), "Pack(Nest(Comp(a, b)))");
        let layout = pad(nest(text("a")), nest(text("b")));
        assert_eq!(shape(&layout), "Nest(Comp(a, b))");
        let layout = pad(nest(text("a")), text("b"));
        assert_eq!(shape(&layout), "Comp(Nest(a), b)");
    }
}
