//! serialize: EdslDoc → Serial (serialize in order to normalize)
//!
//! Flattens the Edsl arena into the flat Serial entry list. The original threaded
//! four accumulator closures (terms, comps, glue, result) plus counters
//! through a CPS recursion on the native stack, which aborts on deep inputs.
//!
//! Here the same computation runs as an explicit left-to-right DFS:
//!
//! - `i`/`j` (group/seq and pack indices) are mutable counters advanced in DFS
//!   pre-order, exactly as the recursion threaded them.
//! - The scoped path accumulators `terms` (nest/pack) and `comps` (grp/seq)
//!   are bump-allocated persistent lists — pushing a wrapper is O(1) and
//!   snapshots (captured by a composition's glue) share structure.
//! - `glue` (how a leaf's term attaches to what follows: Last, Line, or a
//!   Comp separator) is carried per work item.
//!
//! Each leaf emits one `(glue, term)` entry; a final forward pass resolves the
//! entries (in leaf order) into the `SerialEntry` list, computing each
//! composition's scope open/close deltas in the same sweep — byte-identical to
//! the recursive version.

use crate::compiler::types::{
    Attr, Break, EdslDoc, EdslId, EdslNode, Scope, Serial, SerialComp, SerialEntry, Term,
};
use bumpalo::Bump;

/// A nest/pack wrapper accumulated on the path to a term.
#[derive(Copy, Clone)]
enum TermWrap {
    Nest,
    Pack(u64),
}

/// A grp/seq wrapper accumulated on the path to a composition.
#[derive(Copy, Clone)]
enum CompWrap {
    Grp(u64),
    Seq(u64),
}

/// Persistent list of term wrappers; head is the most recently pushed
/// (innermost) wrapper.
struct TermList<'b> {
    wrap: TermWrap,
    next: Option<&'b TermList<'b>>,
}

/// Persistent list of comp wrappers; head is the innermost wrapper. `depth` is
/// the list length (root = 0), so two comps' enclosing lists — which share
/// their outer tail by pointer — can be diffed by an O(delta) longest-common-
/// suffix walk (advance the deeper to equal depth, then step in lockstep to the
/// shared node).
struct CompList<'b> {
    wrap: CompWrap,
    next: Option<&'b CompList<'b>>,
    depth: usize,
}

/// How a leaf's term attaches to the rest of the serial.
#[derive(Copy, Clone)]
enum Glue<'b> {
    /// The final term of the whole document.
    Last,
    /// A hard line break follows.
    Line,
    /// A composition follows, wrapped by the captured comp accumulator.
    Comp {
        comps: Option<&'b CompList<'b>>,
        attr: Attr,
    },
}

/// One emitted leaf: its term and how it glues to what follows.
struct Entry<'b> {
    glue: Glue<'b>,
    term: &'b Term<'b>,
}

/// A pending subtree to visit, with its scoped path state. `i`/`j` are global
/// counters and deliberately not carried here.
struct Work<'b> {
    node: EdslId,
    terms: Option<&'b TermList<'b>>,
    comps: Option<&'b CompList<'b>>,
    glue: Glue<'b>,
    fixed: bool,
}

pub fn serialize<'b, 'a: 'b>(mem: &'b Bump, doc: &'a EdslDoc<'a>) -> Serial<'b> {
    let mut i: u64 = 0;
    let mut j: u64 = 0;
    let mut entries: Vec<Entry<'b>> = Vec::new();

    // Right-to-left visitation is achieved by a stack: pushing the right child
    // before the left makes the left pop (and fully process) first, so the
    // counters thread left-to-right just as the recursion did.
    let mut stack: Vec<Work<'b>> = vec![Work {
        node: doc.root,
        terms: None,
        comps: None,
        glue: Glue::Last,
        fixed: false,
    }];

    while let Some(work) = stack.pop() {
        let Work {
            node,
            terms,
            comps,
            glue,
            fixed,
        } = work;
        match &doc.nodes[node as usize] {
            // A leaf: emit its term with the accumulated wrappers applied.
            leaf @ (EdslNode::Null | EdslNode::Text(_)) => {
                let base = match leaf {
                    EdslNode::Text(data) => Term::Text(data),
                    _ => Term::Null,
                };
                entries.push(Entry {
                    glue,
                    term: apply_terms(mem, terms, mem.alloc(base)),
                });
            }
            EdslNode::Fix(child) => stack.push(Work {
                node: *child,
                terms,
                comps,
                glue,
                fixed: true,
            }),
            // A grp/seq wrapper: push it onto the comp accumulator (assigning
            // the next scope index in DFS pre-order) and descend.
            wrapper @ (EdslNode::Grp(child) | EdslNode::Seq(child)) => {
                let index = i;
                i += 1;
                let wrap = match wrapper {
                    EdslNode::Grp(_) => CompWrap::Grp(index),
                    _ => CompWrap::Seq(index),
                };
                stack.push(Work {
                    node: *child,
                    terms,
                    comps: Some(mem.alloc(CompList {
                        wrap,
                        next: comps,
                        depth: comps.map_or(0, |c| c.depth) + 1,
                    })),
                    glue,
                    fixed,
                });
            }
            // A nest/pack wrapper: push it onto the term accumulator (pack
            // assigning the next pack index in DFS pre-order) and descend.
            wrapper @ (EdslNode::Nest(child) | EdslNode::Pack(child)) => {
                let wrap = match wrapper {
                    EdslNode::Nest(_) => TermWrap::Nest,
                    _ => {
                        let index = j;
                        j += 1;
                        TermWrap::Pack(index)
                    }
                };
                stack.push(Work {
                    node: *child,
                    terms: Some(mem.alloc(TermList { wrap, next: terms })),
                    comps,
                    glue,
                    fixed,
                });
            }
            EdslNode::Line(left, right) => {
                // Right inherits the outer glue; left's trailing term gets a
                // hard line. Push right first so left is processed first.
                stack.push(Work {
                    node: *right,
                    terms,
                    comps,
                    glue,
                    fixed,
                });
                stack.push(Work {
                    node: *left,
                    terms,
                    comps,
                    glue: Glue::Line,
                    fixed,
                });
            }
            EdslNode::Comp(left, right, attr) => {
                // Inside a fix wrapper every composition is fixed.
                let attr1 = Attr {
                    pad: attr.pad,
                    brk: if fixed { Break::Fixed } else { attr.brk },
                };
                let comp_glue = Glue::Comp { comps, attr: attr1 };
                stack.push(Work {
                    node: *right,
                    terms,
                    comps,
                    glue,
                    fixed,
                });
                stack.push(Work {
                    node: *left,
                    terms,
                    comps,
                    glue: comp_glue,
                    fixed,
                });
            }
        }
    }

    // Resolve each leaf entry into a `SerialEntry`, computing every
    // composition's scope open/close deltas in the same forward pass. `prev` is
    // the previous composition's enclosing scope list *on the same line*; it
    // resets at every line break (Line/Last), because grp/seq scopes never cross
    // a hard line — resolve_scopes resolves each line independently. Diffing the
    // shared-tail `CompList`s is O(delta), so this whole pass stays linear even
    // when scopes nest n deep.
    let mut items: Vec<SerialEntry<'b>> = Vec::with_capacity(entries.len());
    let mut prev: Option<&'b CompList<'b>> = None;
    for entry in entries.iter() {
        let item = match entry.glue {
            Glue::Last => {
                prev = None;
                SerialEntry::Last(entry.term)
            }
            Glue::Line => {
                prev = None;
                SerialEntry::Next(entry.term, mem.alloc(SerialComp::Line))
            }
            Glue::Comp { comps, attr } => {
                let (opens, closes) = diff_comps(prev, comps);
                prev = comps;
                let comp = mem.alloc(SerialComp::Comp(
                    attr,
                    mem.alloc_slice_copy(&opens),
                    mem.alloc_slice_copy(&closes),
                ));
                SerialEntry::Next(entry.term, comp)
            }
        };
        items.push(item);
    }
    items
}

/// Diffs two enclosing-scope lists (innermost-first, sharing an outer tail by
/// pointer) into the scopes that *open* (in `cur`, not `prev`) and *close* (in
/// `prev`, not `cur`) at this composition. Order within each list is irrelevant:
/// resolve_scopes keys scopes by index. O(number of scopes that differ).
fn diff_comps<'b>(
    prev: Option<&'b CompList<'b>>,
    cur: Option<&'b CompList<'b>>,
) -> (Vec<Scope>, Vec<Scope>) {
    fn scope_of(wrap: CompWrap) -> Scope {
        match wrap {
            CompWrap::Grp(index) => Scope::Grp(index),
            CompWrap::Seq(index) => Scope::Seq(index),
        }
    }
    fn depth(list: Option<&CompList>) -> usize {
        list.map_or(0, |node| node.depth)
    }
    let mut closes: Vec<Scope> = Vec::new();
    let mut opens: Vec<Scope> = Vec::new();
    let mut a = prev; // contributes closes
    let mut b = cur; // contributes opens
    let (mut da, mut db) = (depth(a), depth(b));
    // Drop the deeper list's excess head down to the shallower list's depth.
    while da > db {
        let node = a.expect("depth > 0");
        closes.push(scope_of(node.wrap));
        a = node.next;
        da -= 1;
    }
    while db > da {
        let node = b.expect("depth > 0");
        opens.push(scope_of(node.wrap));
        b = node.next;
        db -= 1;
    }
    // Equal depth: step in lockstep until the shared tail (same node pointer,
    // or both empty) — everything above it differs.
    loop {
        match (a, b) {
            (None, None) => break,
            (Some(x), Some(y)) if std::ptr::eq(x, y) => break,
            (Some(x), Some(y)) => {
                closes.push(scope_of(x.wrap));
                opens.push(scope_of(y.wrap));
                a = x.next;
                b = y.next;
            }
            _ => unreachable!("equal-depth lists reach the shared tail together"),
        }
    }
    (opens, closes)
}

/// Applies the accumulated term wrappers to a base term. The list head is the
/// innermost wrapper, so folding head-to-tail wraps from the inside out.
fn apply_terms<'b>(
    mem: &'b Bump,
    list: Option<&'b TermList<'b>>,
    base: &'b Term<'b>,
) -> &'b Term<'b> {
    let mut term = base;
    let mut cur = list;
    while let Some(node) = cur {
        term = match node.wrap {
            TermWrap::Nest => mem.alloc(Term::Nest(term)),
            TermWrap::Pack(index) => mem.alloc(Term::Pack(index, term)),
        };
        cur = node.next;
    }
    term
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{Pad, push_node};

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    /// Wraps a `Text` leaf in `DEEP` layers of `wrap`.
    fn deep_unary(text: &'static str, wrap: fn(EdslId) -> EdslNode<'static>) -> EdslDoc<'static> {
        let mut nodes: Vec<EdslNode> = Vec::new();
        let mut cur = push_node(&mut nodes, EdslNode::Text(text));
        for _ in 0..DEEP {
            cur = push_node(&mut nodes, wrap(cur));
        }
        EdslDoc { nodes, root: cur }
    }

    #[test]
    fn serialize_handles_deep_comp_chain() {
        let attr = Attr {
            pad: Pad::Unpadded,
            brk: Break::Breakable,
        };
        // Right-nested Comp chain of DEEP compositions over DEEP + 1 texts.
        let mut nodes: Vec<EdslNode> = Vec::new();
        let mut cur = push_node(&mut nodes, EdslNode::Text("z"));
        for _ in 0..DEEP {
            let left = push_node(&mut nodes, EdslNode::Text("y"));
            cur = push_node(&mut nodes, EdslNode::Comp(left, cur, attr));
        }
        let doc = EdslDoc { nodes, root: cur };
        let mem = Bump::new();
        let serial = serialize(&mem, &doc);
        // DEEP Next entries, then a final Last entry.
        let count = serial
            .iter()
            .filter(|e| matches!(e, SerialEntry::Next(..)))
            .count();
        assert_eq!(count, DEEP);
        assert!(matches!(serial.last(), Some(SerialEntry::Last(_))));
    }

    #[test]
    fn serialize_handles_deep_nest_chain() {
        let doc = deep_unary("x", EdslNode::Nest);
        let mem = Bump::new();
        let serial = serialize(&mem, &doc);
        // Single leaf: one Last carrying a Nest^DEEP term.
        let [SerialEntry::Last(term)] = serial[..] else {
            panic!("expected a single Last")
        };
        let mut count = 0usize;
        let mut cur: &Term = term;
        while let Term::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn serialize_handles_deep_pack_chain_indices() {
        let doc = deep_unary("x", EdslNode::Pack);
        let mem = Bump::new();
        let serial = serialize(&mem, &doc);
        let [SerialEntry::Last(term)] = serial[..] else {
            panic!("expected a single Last")
        };
        // The outermost Pack is entered first and gets index 0; indices then
        // increase inward.
        let mut expected = 0u64;
        let mut cur: &Term = term;
        while let Term::Pack(index, inner) = cur {
            assert_eq!(*index, expected);
            expected += 1;
            cur = inner;
        }
        assert_eq!(expected as usize, DEEP);
    }

    #[test]
    fn serialize_handles_deep_grp_chain() {
        // Deep grp nesting exercises the i counter and CompList/stack depth.
        let doc = deep_unary("x", EdslNode::Grp);
        let mem = Bump::new();
        // Should not overflow; a single leaf yields a trivial one-Last serial.
        let serial = serialize(&mem, &doc);
        assert!(matches!(serial[..], [SerialEntry::Last(_)]));
    }
}
