//! resolve_scopes: FixedDoc → RebuildDoc (solve the grp/seq scopes per line)
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
//!    `rebuild` depend on.
//! 2. [`Graph::solve`] resolves nodes that both close and open scopes (a
//!    run that straddles a scope boundary, as in `grp(a + b) !& c`) by
//!    widening: leading seq edges out of the node are re-sourced onto the
//!    incoming side, and the incoming edges are handed forward past the
//!    first grp edge out.
//! 3. [`rebuild`] reads each line back as a composition spine with grp/seq
//!    wrappers, using a stack of open scopes.
//!
//! The graph is a side table over the `FixedDoc`'s item buffer (a node *is*
//! its item's id) plus one edge arena. A node's incident edges are intrusive
//! linked lists threaded through the edge arena, so `solve`'s surgery (pop a
//! list head, insert before a known edge, splice one list into another) is
//! O(1) pointer rewiring and building the graph allocates nothing per node
//! or edge.

use crate::arena::{Arena, Id, IdVec};
use crate::layout::Pad;
use crate::serialize::{FixedComp, FixedDoc, FixedLine, Run, ScopeKind};

pub(crate) type ObjId<'a> = Id<Obj<'a>>;

/// An object in a per-line composition tree. A run is a leaf (its terms are
/// ranges into the borrowed `FixedDoc`); a composition's left operand is
/// always a leaf or a wrapper, never another composition.
#[derive(Debug, Copy, Clone)]
pub(crate) enum Obj<'a> {
    Run(Run<'a>),
    Grp(ObjId<'a>),
    Seq(ObjId<'a>),
    Comp(ObjId<'a>, ObjId<'a>, Pad),
}

/// The document rebuilt as one composition tree per line, over runs whose
/// terms still carry their nest/pack paths. The arena is postorder (children
/// precede parents), so consumers fold it with a forward loop.
#[derive(Debug)]
pub(crate) struct RebuildDoc<'a> {
    /// One root object per line, in document order.
    pub(crate) lines: Vec<ObjId<'a>>,
    pub(crate) objs: Arena<Obj<'a>>,
}

pub(crate) fn resolve_scopes<'a>(doc: &FixedDoc<'a>) -> RebuildDoc<'a> {
    let mut graph = Graph::build(doc);
    graph.solve();
    rebuild(doc, &graph)
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
    /// Edges sourced at this node, in list order (solve and rebuild depend on
    /// the order).
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

// --- Rebuilding the composition spine --------------------------------------

/// Reads the solved graph back, line by line, into a flat postorder arena.
///
/// A line is walked left to right threading a partial spine — the objects
/// composed so far at the current nesting, with the pad after each — and a
/// stack of open scopes, each remembering where its own partial spine
/// begins. A node that opens scopes pushes one entry per out-edge (in list
/// order) before joining the spine; a node that closes scopes folds the
/// spine above each closed scope's start onto itself, wraps it, and pops.
/// After `solve` no node does both.
fn rebuild<'a>(doc: &FixedDoc<'a>, g: &Graph<'a>) -> RebuildDoc<'a> {
    // Every node yields at least one object, so the item total is a capacity
    // floor for the object arena.
    let mut objs: Arena<Obj<'a>> = Arena::with_capacity(doc.items.len());
    let mut partials: Vec<(ObjId<'a>, Pad)> = Vec::new();
    let mut open: Vec<(ScopeKind, usize)> = Vec::new();
    let lines = doc
        .lines
        .iter()
        .map(|line| visit_line(doc, g, line, &mut objs, &mut partials, &mut open))
        .collect();
    RebuildDoc { lines, objs }
}

fn visit_line<'a>(
    doc: &FixedDoc<'a>,
    g: &Graph<'a>,
    line: &FixedLine<'a>,
    objs: &mut Arena<Obj<'a>>,
    partials: &mut Vec<(ObjId<'a>, Pad)>,
    open: &mut Vec<(ScopeKind, usize)>,
) -> ObjId<'a> {
    partials.clear();
    open.clear();
    let items = line.items.slice(&doc.items);
    // `seps[i].pad` is the pad between item `i` and `i + 1`.
    let seps = line.seps.slice(&doc.item_seps);
    for (i, run) in items.iter().enumerate() {
        let node = &g.nodes[line.items.id_at(i)];
        let mut obj = objs.push(Obj::Run(*run));
        assert!(
            node.ins_len == 0 || node.outs_head.is_none(),
            "solve leaves no node both closing and opening scopes"
        );
        // Close the scopes ending here, innermost first: fold this scope's
        // spine onto the object, wrap it, and resume the enclosing spine.
        for _ in 0..node.ins_len {
            let (kind, start) = open.pop().expect("a closing scope is open");
            obj = compose(objs, &partials[start..], obj);
            partials.truncate(start);
            obj = objs.push(match kind {
                ScopeKind::Grp => Obj::Grp(obj),
                ScopeKind::Seq => Obj::Seq(obj),
            });
        }
        // Open the scopes starting here, outermost first: each begins its
        // own spine with this object.
        let mut e = node.outs_head;
        while let Some(edge) = e {
            open.push((g.edges[edge].kind, partials.len()));
            e = g.edges[edge].next_out;
        }
        match seps.get(i) {
            Some(sep) => partials.push((obj, sep.pad)),
            None => {
                assert!(open.is_empty(), "every scope closes by the end of its line");
                return compose(objs, partials, obj);
            }
        }
    }
    unreachable!("every line has at least one item")
}

/// Folds a partial spine onto `obj`, innermost element first:
/// `[(x0, p0), .., (xk, pk)]` yields `Comp(x0, Comp(.., Comp(xk, obj, pk)), p0)`.
fn compose<'a>(
    objs: &mut Arena<Obj<'a>>,
    partial: &[(ObjId<'a>, Pad)],
    obj: ObjId<'a>,
) -> ObjId<'a> {
    let mut result = obj;
    for &(left, pad) in partial.iter().rev() {
        result = objs.push(Obj::Comp(left, result, pad));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructors::{comp, fix, grp, seq, text};
    use crate::layout::{Break, Layout};
    use crate::serialize::serialize;

    /// The rebuilt tree of a one-line layout, printed as nested constructor
    /// names over the texts; a run of several terms prints as `Fix(a b c)`.
    fn shape(layout: &Layout) -> String {
        let fixed = serialize(&layout.nodes, &layout.text);
        let out = resolve_scopes(&fixed);
        let [root] = out.lines[..] else {
            panic!("expected one line")
        };
        fn obj<'a>(fixed: &FixedDoc<'a>, out: &RebuildDoc<'a>, id: ObjId<'a>) -> String {
            match out.objs[id] {
                Obj::Run(run) => {
                    let texts: Vec<&str> = run
                        .terms
                        .slice(&fixed.terms)
                        .iter()
                        .map(|t| t.text)
                        .collect();
                    match texts[..] {
                        [one] => one.to_string(),
                        _ => format!("Fix({})", texts.join(" ")),
                    }
                }
                Obj::Grp(c) => format!("Grp({})", obj(fixed, out, c)),
                Obj::Seq(c) => format!("Seq({})", obj(fixed, out, c)),
                Obj::Comp(l, r, _) => {
                    format!("Comp({}, {})", obj(fixed, out, l), obj(fixed, out, r))
                }
            }
        }
        obj(&fixed, &out, root)
    }

    fn pad(l: Layout, r: Layout) -> Layout {
        comp(l, r, Pad::Padded, Break::Breakable)
    }

    fn fixed(l: Layout, r: Layout) -> Layout {
        comp(l, r, Pad::Padded, Break::Fixed)
    }

    #[test]
    fn nested_scopes_are_rebuilt_as_wrappers() {
        // The tree structure survives the round trip through the item list.
        let layout = pad(grp(pad(text("a"), text("b"))), text("c"));
        assert_eq!(shape(&layout), "Comp(Grp(Comp(a, b)), c)");
        let layout = seq(pad(text("a"), grp(pad(text("b"), text("c")))));
        assert_eq!(shape(&layout), "Seq(Comp(a, Grp(Comp(b, c))))");
    }

    #[test]
    fn scopes_opening_together_nest_in_opening_order() {
        // grp(seq(a + b)) + c: both scopes open at a and close at b; the
        // grp, opened first, wraps the seq.
        let layout = pad(grp(seq(pad(text("a"), text("b")))), text("c"));
        assert_eq!(shape(&layout), "Comp(Grp(Seq(Comp(a, b))), c)");
    }

    #[test]
    fn scope_widens_to_cover_a_run_that_straddles_its_end() {
        // grp(a + b) !+ c: the fixed composition coalesces b and c into one
        // run, so the grp cannot end between them; it widens to include c.
        let layout = fixed(grp(pad(text("a"), text("b"))), text("c"));
        assert_eq!(shape(&layout), "Grp(Comp(a, Fix(b c)))");
    }

    #[test]
    fn straddled_scopes_resolve_seq_outward_and_grp_inward() {
        // seq(a + b) !+ grp(c + d): the run [b c] both closes the seq and
        // opens the grp. The seq's end is handed past the grp, so the seq
        // covers everything and the grp sits inside it.
        let layout = fixed(
            seq(pad(text("a"), text("b"))),
            grp(pad(text("c"), text("d"))),
        );
        assert_eq!(shape(&layout), "Seq(Comp(a, Grp(Comp(Fix(b c), d))))");
    }

    #[test]
    fn a_fix_is_one_run() {
        let layout = fix(pad(pad(text("a"), text("b")), text("c")));
        assert_eq!(shape(&layout), "Fix(a b c)");
    }

    #[test]
    fn scopes_interleave_with_plain_items() {
        // a + grp(b + c) + d + grp(e + f) + g: the spine resumes after each
        // closed scope.
        let layout = pad(
            text("a"),
            pad(
                grp(pad(text("b"), text("c"))),
                pad(text("d"), pad(grp(pad(text("e"), text("f"))), text("g"))),
            ),
        );
        assert_eq!(
            shape(&layout),
            "Comp(a, Comp(Grp(Comp(b, c)), Comp(d, Comp(Grp(Comp(e, f)), g))))"
        );
    }
}
