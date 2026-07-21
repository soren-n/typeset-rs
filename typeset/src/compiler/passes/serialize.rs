//! Pass 2: Edsl → Serial (serialize in order to normalize)
//!
//! Flattens the Edsl tree into a linear Serial spine. The original threaded
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
//! Each leaf emits one `(glue, term)` entry; the entries, in leaf order, are
//! folded from the right onto `Past` to build the Serial — byte-identical to
//! the recursive version.

use crate::compiler::types::{Attr, Edsl, Scope, Serial, SerialComp, SerialEntry, Term};
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
struct Work<'b, 'a> {
    layout: &'a Edsl<'a>,
    terms: Option<&'b TermList<'b>>,
    comps: Option<&'b CompList<'b>>,
    glue: Glue<'b>,
    fixed: bool,
}

pub fn serialize<'b, 'a: 'b>(mem: &'b Bump, layout: &'a Edsl<'a>) -> Serial<'b> {
    let mut i: u64 = 0;
    let mut j: u64 = 0;
    let mut entries: Vec<Entry<'b>> = Vec::new();

    // Right-to-left visitation is achieved by a stack: pushing the right child
    // before the left makes the left pop (and fully process) first, so the
    // counters thread left-to-right just as the recursion did.
    let mut stack: Vec<Work<'b, 'a>> = vec![Work {
        layout,
        terms: None,
        comps: None,
        glue: Glue::Last,
        fixed: false,
    }];

    while let Some(work) = stack.pop() {
        let Work {
            layout,
            terms,
            comps,
            glue,
            fixed,
        } = work;
        match layout {
            Edsl::Null => {
                entries.push(Entry {
                    glue,
                    term: apply_terms(mem, terms, mem.alloc(Term::Null)),
                });
            }
            Edsl::Text(data) => {
                entries.push(Entry {
                    glue,
                    term: apply_terms(mem, terms, mem.alloc(Term::Text(data))),
                });
            }
            Edsl::Fix(layout1) => stack.push(Work {
                layout: layout1,
                terms,
                comps,
                glue,
                fixed: true,
            }),
            Edsl::Grp(layout1) => {
                let index = i;
                i += 1;
                stack.push(Work {
                    layout: layout1,
                    terms,
                    comps: Some(mem.alloc(CompList {
                        wrap: CompWrap::Grp(index),
                        next: comps,
                        depth: comps.map_or(0, |c| c.depth) + 1,
                    })),
                    glue,
                    fixed,
                });
            }
            Edsl::Seq(layout1) => {
                let index = i;
                i += 1;
                stack.push(Work {
                    layout: layout1,
                    terms,
                    comps: Some(mem.alloc(CompList {
                        wrap: CompWrap::Seq(index),
                        next: comps,
                        depth: comps.map_or(0, |c| c.depth) + 1,
                    })),
                    glue,
                    fixed,
                });
            }
            Edsl::Nest(layout1) => stack.push(Work {
                layout: layout1,
                terms: Some(mem.alloc(TermList {
                    wrap: TermWrap::Nest,
                    next: terms,
                })),
                comps,
                glue,
                fixed,
            }),
            Edsl::Pack(layout1) => {
                let index = j;
                j += 1;
                stack.push(Work {
                    layout: layout1,
                    terms: Some(mem.alloc(TermList {
                        wrap: TermWrap::Pack(index),
                        next: terms,
                    })),
                    comps,
                    glue,
                    fixed,
                });
            }
            Edsl::Line(left, right) => {
                // Right inherits the outer glue; left's trailing term gets a
                // hard line. Push right first so left is processed first.
                stack.push(Work {
                    layout: right,
                    terms,
                    comps,
                    glue,
                    fixed,
                });
                stack.push(Work {
                    layout: left,
                    terms,
                    comps,
                    glue: Glue::Line,
                    fixed,
                });
            }
            Edsl::Comp(left, right, attr) => {
                let attr1 = Attr {
                    pad: attr.pad,
                    fix: fixed || attr.fix,
                };
                let comp_glue = Glue::Comp { comps, attr: attr1 };
                stack.push(Work {
                    layout: right,
                    terms,
                    comps,
                    glue,
                    fixed,
                });
                stack.push(Work {
                    layout: left,
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
    // a hard line — structurize resolves each line independently. Diffing the
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
    mem.alloc_slice_copy(&items)
}

/// Diffs two enclosing-scope lists (innermost-first, sharing an outer tail by
/// pointer) into the scopes that *open* (in `cur`, not `prev`) and *close* (in
/// `prev`, not `cur`) at this composition. Order within each list is irrelevant:
/// structurize keys scopes by index. O(number of scopes that differ).
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

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    #[test]
    fn serialize_handles_deep_comp_chain() {
        let mem = Bump::new();
        let attr = Attr {
            pad: false,
            fix: false,
        };
        // Right-nested Comp chain of DEEP compositions over DEEP + 1 texts.
        let mut edsl: &Edsl = mem.alloc(Edsl::Text("z"));
        for _ in 0..DEEP {
            edsl = mem.alloc(Edsl::Comp(mem.alloc(Edsl::Text("y")), edsl, attr));
        }
        let serial = serialize(&mem, edsl);
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
        let mem = Bump::new();
        let mut edsl: &Edsl = mem.alloc(Edsl::Text("x"));
        for _ in 0..DEEP {
            edsl = mem.alloc(Edsl::Nest(edsl));
        }
        let serial = serialize(&mem, edsl);
        // Single leaf: one Last carrying a Nest^DEEP term.
        let [SerialEntry::Last(term)] = serial else {
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
        let mem = Bump::new();
        let mut edsl: &Edsl = mem.alloc(Edsl::Text("x"));
        for _ in 0..DEEP {
            edsl = mem.alloc(Edsl::Pack(edsl));
        }
        let serial = serialize(&mem, edsl);
        let [SerialEntry::Last(term)] = serial else {
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
        let mem = Bump::new();
        // Deep grp nesting exercises the i counter and CompList/stack depth.
        let mut edsl: &Edsl = mem.alloc(Edsl::Text("x"));
        for _ in 0..DEEP {
            edsl = mem.alloc(Edsl::Grp(edsl));
        }
        // Should not overflow; a single leaf yields a trivial one-Last serial.
        let serial = serialize(&mem, edsl);
        assert!(matches!(serial, [SerialEntry::Last(_)]));
    }
}
