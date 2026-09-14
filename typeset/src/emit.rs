//! emit: solved lines → Doc
//!
//! After `solve`, a line's scopes nest: every node either closes scopes or
//! opens them, never both, and the scopes open at any item form a stack. So
//! a line reads back as a tree of *spines* — the line's own, and one per
//! scope — each a left-to-right sequence of elements (an item, or a nested
//! scope) with a pad between neighbours. The line is walked twice with a
//! stack of open spines, applying on the way the rules the reference runs
//! as five tree rewrites afterwards (null removal, seq identities, grp
//! identities, reassociation, rescoping):
//!
//! - A counting walk decides which seqs survive. Empty items vanish; a scope
//!   with no surviving element vanishes with them. A seq is kept when it
//!   groups two or more compositions (a grp beneath it is opaque, a seq is
//!   transparent) and is not directly under a seq.
//! - The emitting walk lowers each run, threads the pad between surviving
//!   neighbours (a vanished element's pads merge into the one composition
//!   that remains; a spine's leading and trailing pads are dropped), decides
//!   the grps (dropped when they group no composition, absorbed at the head
//!   of their enclosing spine; seqs are transparent to the count), composes
//!   each surviving spine right-nested with the nest/pack prefix its
//!   operands share factored out, and splices a dropped scope's elements
//!   into the spine that encloses it.
//!
//! The rules are not confluent, so the order is part of the semantics, and
//! the two walks reproduce it: seq survival never depends on a grp
//! decision, while grp absorption depends on which seqs survive.
//!
//! Nothing crosses a hard line, so [`Emitter`] consumes one line at a time
//! and every per-line structure — the graph, the spine stacks — is scratch
//! reused across lines.

use crate::arena::Range;
use crate::doc::{Doc, ObjId, ObjNode};
use crate::graph::{EdgeId, Graph};
use crate::layout::{Break, Pad};
use crate::lines::{Event, Indent, Line, PathId, Paths, Scope};

/// The pass: consumes lines one at a time into a [`Doc`].
pub(crate) struct Emitter<'a> {
    graph: Graph<'a>,
    // Scratch, reused across lines.
    counting: Vec<Counting<'a>>,
    spines: Vec<Spine>,
    /// The elements of every open spine, each with the pad before it (the
    /// first element of a spine ignores its pad).
    elements: Vec<(Pad, Atom)>,
    doc: Doc,
}

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

/// A lowered element: the innermost nest/pack wrapper still to be applied
/// around its object (a node of the path tree, `None` for none).
type Atom = (Option<PathId>, ObjId);

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

impl<'a> Emitter<'a> {
    /// `capacity` is a hint for the object arena: the layout's node count
    /// is about right.
    pub(crate) fn new(capacity: usize) -> Self {
        Emitter {
            graph: Graph::new(),
            counting: Vec::new(),
            spines: Vec::new(),
            elements: Vec::new(),
            doc: Doc::with_capacity(capacity),
        }
    }

    /// Appends `line` to the document.
    pub(crate) fn push_line(&mut self, line: &Line<'_, 'a>) {
        self.graph.build(line);
        self.graph.solve();
        self.count_line(line);
        let root = self.emit_line(line);
        self.doc.lines.push(root);
    }

    pub(crate) fn finish(self) -> Doc {
        self.doc
    }

    /// The counting walk: decides `kept` for every seq edge of the line.
    fn count_line(&mut self, line: &Line<'_, 'a>) {
        self.counting.clear();
        self.counting.push(Counting {
            edge: None,
            seq: false,
            elements: 0,
            inner: Count::Zero,
        });
        for id in self.graph.nodes.ids() {
            let node = self.graph.nodes[id];
            // The item is the first element of every scope opening here.
            let mut e = node.outs_head;
            while let Some(edge) = e {
                self.counting.push(Counting {
                    edge: Some(edge),
                    seq: self.graph.edges[edge].kind == Scope::Seq,
                    elements: 0,
                    inner: Count::Zero,
                });
                e = self.graph.edges[edge].next_out;
            }
            // ... and the last element of every scope closing here,
            // innermost first; each closed scope is then an element of the
            // next.
            let mut survives = alive(line, node.events);
            for _ in 0..node.ins_len {
                let mut top = self.counting.pop().expect("a closing scope is open");
                top.elements += usize::from(survives);
                let count = Count::between(top.elements).add(top.inner);
                let parent = self.counting.last_mut().expect("the line is never popped");
                if top.seq {
                    let edge = top.edge.expect("only scopes close");
                    self.graph.edges[edge].kept = !parent.seq && count == Count::Many;
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
    fn emit_line(&mut self, line: &Line<'_, 'a>) -> Option<ObjId> {
        self.spines.clear();
        self.elements.clear();
        self.spines.push(Spine {
            wrap: Wrap::Line,
            start: 0,
            head: true,
            pending: None,
            inner: Count::Zero,
        });
        for id in self.graph.nodes.ids() {
            let node = self.graph.nodes[id];
            assert!(
                node.ins_len == 0 || node.outs_head.is_none(),
                "solve leaves no node both closing and opening scopes"
            );
            let mut e = node.outs_head;
            while let Some(edge) = e {
                self.open(edge);
                e = self.graph.edges[edge].next_out;
            }
            let mut element = self.emit_run(line, node.events);
            for _ in 0..node.ins_len {
                self.push(element);
                element = self.close(line.paths);
            }
            self.push(element);
            // The pad between this item and the next.
            match line.events.get(node.events.end()) {
                None => {}
                Some(&Event::Comp(pad, Break::Breakable)) => {
                    let top = self.spines.last_mut().expect("the line is never popped");
                    top.pending = top.pending.map(|p| p.merge(pad));
                }
                Some(other) => {
                    unreachable!("an item ends at a breakable composition, found {other:?}")
                }
            }
        }
        let top = self.spines.pop().expect("the line spine");
        assert!(
            self.spines.is_empty() && top.wrap == Wrap::Line,
            "every scope closes by the end of its line"
        );
        self.compose(line.paths, top.start)
            .map(|(path, obj)| wrap_path(&mut self.doc, line.paths, path, None, obj))
    }

    /// Opens the scope of `edge`: pushes its spine.
    fn open(&mut self, edge: EdgeId<'a>) {
        let parent = self.spines.last().expect("the line is never popped");
        // The scope is at the head of its enclosing group when it is the
        // first surviving element of a spine that is itself at the head.
        let head = parent.head && parent.pending.is_none();
        let edge = &self.graph.edges[edge];
        let (wrap, head) = match edge.kind {
            Scope::Grp if head => (Wrap::Splice, true),
            Scope::Grp => (Wrap::Grp, false),
            Scope::Seq if edge.kept => (Wrap::Seq, false),
            Scope::Seq => (Wrap::Splice, head),
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
    fn close(&mut self, paths: &Paths) -> Option<Atom> {
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
                let (path, obj) = self.compose(paths, top.start)?;
                let obj = self.doc.push(match wrap {
                    Wrap::Grp => ObjNode::Grp(obj),
                    _ => ObjNode::Seq(obj),
                });
                Some((path, obj))
            }
        }
    }

    /// Composes the elements from `start` on as one right-nested spine,
    /// factoring at each composition the nest/pack wrappers its operands
    /// share — the chain of their paths' lowest common ancestor — and
    /// removes them. Returns the spine's remaining path and object, or
    /// nothing for an empty spine.
    fn compose(&mut self, paths: &Paths, start: usize) -> Option<Atom> {
        let elements = &self.elements[start..];
        let (mut res_path, mut result) = elements.last()?.1;
        // Innermost first, so each composition sees its right operand's
        // leftover wrappers.
        for k in (1..elements.len()).rev() {
            let (l_path, left) = elements[k - 1].1;
            let pad = elements[k].0;
            let shared = paths.tree.lca(l_path, res_path);
            let left = wrap_path(&mut self.doc, paths, l_path, shared, left);
            let right = wrap_path(&mut self.doc, paths, res_path, shared, result);
            result = self.doc.push(ObjNode::Comp(left, right, pad));
            res_path = shared;
        }
        self.elements.truncate(start);
        Some((res_path, result))
    }

    /// Lowers a run: drops its empty texts, merges the pads between
    /// survivors (a dropped text's padding on either side folds into the one
    /// composition that remains), and keeps the first surviving text's path
    /// as the run's. `None` if nothing survived.
    fn emit_run(&mut self, line: &Line<'_, 'a>, events: Range<Event<'a>>) -> Option<Atom> {
        let mut first: Option<Option<PathId>> = None;
        // The pad to put before the next survivor: `None` until the first
        // survivor (its leading pads are dropped), then the merge of every
        // pad since the previous survivor.
        let mut pending: Option<Pad> = None;
        let start = self.doc.start_run();
        for event in events.slice(line.events) {
            match *event {
                Event::Text { path, text } if !text.is_empty() => {
                    self.doc.push_text(pending.unwrap_or(Pad::Unpadded), text);
                    first.get_or_insert(path);
                    pending = Some(Pad::Unpadded);
                }
                Event::Text { .. } | Event::Open(_) | Event::Close => {}
                Event::Comp(pad, Break::Fixed) => {
                    if let Some(p) = pending.as_mut() {
                        *p = p.merge(pad);
                    }
                }
                Event::Comp(_, Break::Breakable) => {
                    unreachable!("a run has no breakable composition")
                }
            }
        }
        Some((first?, self.doc.end_run(start)))
    }
}

/// Whether a run has any non-empty text.
fn alive<'a>(line: &Line<'_, 'a>, events: Range<Event<'a>>) -> bool {
    events
        .slice(line.events)
        .iter()
        .any(|event| matches!(event, Event::Text { text, .. } if !text.is_empty()))
}

/// Wraps `obj` in the wrappers from `path` outward up to (not including)
/// `upto`, innermost first, returning the outermost wrapper.
fn wrap_path(
    doc: &mut Doc,
    paths: &Paths,
    path: Option<PathId>,
    upto: Option<PathId>,
    obj: ObjId,
) -> ObjId {
    paths
        .tree
        .ancestors(path, upto)
        .fold(obj, |obj, id| match paths.tree[id].value {
            Indent::Nest => doc.push(ObjNode::Nest(obj)),
            Indent::Pack(index) => doc.push(ObjNode::Pack(index, obj)),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructors::{comp, fix, grp, nest, null, pack, seq, text};
    use crate::layout::{Break, Layout};

    /// The document of a one-line layout, printed as nested constructor
    /// names over the runs' texts.
    fn shape(layout: Layout) -> String {
        let doc = layout.compile();
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
    fn nested_scopes_become_wrappers() {
        let layout = pad(text("x"), pad(grp(pad(text("a"), text("b"))), text("c")));
        assert_eq!(shape(layout), "Comp(x, Comp(Grp(Comp(a, b)), c))");
        let layout = seq(pad(
            text("a"),
            pad(text("b"), grp(pad(text("c"), text("d")))),
        ));
        assert_eq!(shape(layout), "Seq(Comp(a, Comp(b, Grp(Comp(c, d)))))");
    }

    #[test]
    fn scopes_opening_together_nest_in_opening_order() {
        // x + grp(seq(a + b + c)): both scopes open at a and close at c; the
        // grp, opened first, wraps the seq.
        let layout = pad(
            text("x"),
            grp(seq(pad(text("a"), pad(text("b"), text("c"))))),
        );
        assert_eq!(shape(layout), "Comp(x, Grp(Seq(Comp(a, Comp(b, c)))))");
    }

    #[test]
    fn scope_widens_to_cover_a_run_that_straddles_its_end() {
        // x + (grp(a + b) !+ c): the fixed composition coalesces b and c
        // into one run, so the grp cannot end between them; it widens to
        // include c.
        let layout = pad(text("x"), fixed(grp(pad(text("a"), text("b"))), text("c")));
        assert_eq!(shape(layout), "Comp(x, Grp(Comp(a, b c)))");
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
        assert_eq!(shape(layout), "Seq(Comp(a, Comp(b, Grp(Comp(c d, e)))))");
    }

    #[test]
    fn a_fix_is_one_run() {
        let layout = fix(pad(pad(text("a"), text("b")), text("c")));
        assert_eq!(shape(layout), "a b c");
    }

    #[test]
    fn spines_are_right_nested_across_dropped_scopes() {
        // (a + b) + grp(c) + d: the grp groups nothing and is dropped; the
        // whole line is one right-nested spine.
        let layout = pad(pad(text("a"), text("b")), pad(grp(text("c")), text("d")));
        assert_eq!(shape(layout), "Comp(a, Comp(b, Comp(c, d)))");
    }

    #[test]
    fn a_grp_at_the_head_of_its_group_is_absorbed() {
        assert_eq!(
            shape(pad(grp(pad(text("a"), text("b"))), text("c"))),
            "Comp(a, Comp(b, c))"
        );
        // Inside a kept seq the head resets, so the grp survives.
        assert_eq!(
            shape(seq(pad(
                grp(pad(text("a"), text("b"))),
                pad(text("c"), text("d"))
            ))),
            "Seq(Comp(Grp(Comp(a, b)), Comp(c, d)))"
        );
    }

    #[test]
    fn a_seq_needs_two_compositions_and_no_seq_above_it() {
        assert_eq!(shape(seq(pad(text("a"), text("b")))), "Comp(a, b)");
        assert_eq!(
            shape(seq(pad(text("a"), seq(pad(text("b"), text("c")))))),
            "Seq(Comp(a, Comp(b, c)))"
        );
        // A grp beneath a seq is opaque to its count.
        assert_eq!(
            shape(pad(
                text("x"),
                seq(grp(pad(text("a"), pad(text("b"), text("c")))))
            )),
            "Comp(x, Grp(Comp(a, Comp(b, c))))"
        );
    }

    #[test]
    fn empty_texts_vanish_and_their_pads_merge() {
        assert_eq!(shape(null()), "Empty");
        assert_eq!(shape(pad(nest(null()), text("a"))), "a");
        // The pads around a vanished middle element merge; a vanished
        // leading element's pad is dropped even across a wrapper.
        let layout = comp(
            text("a"),
            comp(null(), text("b"), Pad::Padded, Break::Breakable),
            Pad::Unpadded,
            Break::Breakable,
        );
        let doc = layout.compile();
        let root = doc.lines[0].expect("a survives");
        assert!(matches!(doc.objs[root], ObjNode::Comp(_, _, Pad::Padded)));
        let layout = comp(
            text("a"),
            grp(comp(null(), text("b"), Pad::Padded, Break::Breakable)),
            Pad::Unpadded,
            Break::Breakable,
        );
        let doc = layout.compile();
        let root = doc.lines[0].expect("a survives");
        assert!(matches!(doc.objs[root], ObjNode::Comp(_, _, Pad::Unpadded)));
    }

    #[test]
    fn shared_wrapper_prefixes_are_factored_out() {
        let layout = pack(nest(pad(text("a"), text("b"))));
        assert_eq!(shape(layout), "Pack(Nest(Comp(a, b)))");
        let layout = pad(nest(text("a")), nest(text("b")));
        assert_eq!(shape(layout), "Nest(Comp(a, b))");
        let layout = pad(nest(text("a")), text("b"));
        assert_eq!(shape(layout), "Comp(Nest(a), b)");
    }
}
