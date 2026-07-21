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

use crate::compiler::types::{Attr, Edsl, Serial, SerialComp, SerialTerm};
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

/// Persistent list of comp wrappers; head is the innermost wrapper.
struct CompList<'b> {
    wrap: CompWrap,
    next: Option<&'b CompList<'b>>,
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
    term: &'b SerialTerm<'b>,
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

pub fn serialize<'b, 'a: 'b>(mem: &'b Bump, layout: &'a Edsl<'a>) -> &'b Serial<'b> {
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
                    term: apply_terms(mem, terms, mem.alloc(SerialTerm::Null)),
                });
            }
            Edsl::Text(data) => {
                entries.push(Entry {
                    glue,
                    term: apply_terms(mem, terms, mem.alloc(SerialTerm::Text(data))),
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

    // Fold the leaf entries (document order) from the right onto Past.
    let mut serial: &'b Serial<'b> = mem.alloc(Serial::Past);
    for entry in entries.iter().rev() {
        serial = match entry.glue {
            Glue::Last => mem.alloc(Serial::Last(entry.term, serial)),
            Glue::Line => mem.alloc(Serial::Next(
                entry.term,
                mem.alloc(SerialComp::Line),
                serial,
            )),
            Glue::Comp { comps, attr } => {
                let comp = apply_comps(mem, comps, mem.alloc(SerialComp::Comp(attr)));
                mem.alloc(Serial::Next(entry.term, comp, serial))
            }
        };
    }
    serial
}

/// Applies the accumulated term wrappers to a base term. The list head is the
/// innermost wrapper, so folding head-to-tail wraps from the inside out.
fn apply_terms<'b>(
    mem: &'b Bump,
    list: Option<&'b TermList<'b>>,
    base: &'b SerialTerm<'b>,
) -> &'b SerialTerm<'b> {
    let mut term = base;
    let mut cur = list;
    while let Some(node) = cur {
        term = match node.wrap {
            TermWrap::Nest => mem.alloc(SerialTerm::Nest(term)),
            TermWrap::Pack(index) => mem.alloc(SerialTerm::Pack(index, term)),
        };
        cur = node.next;
    }
    term
}

/// Applies the accumulated comp wrappers to a base comp (innermost first).
fn apply_comps<'b>(
    mem: &'b Bump,
    list: Option<&'b CompList<'b>>,
    base: &'b SerialComp<'b>,
) -> &'b SerialComp<'b> {
    let mut comp = base;
    let mut cur = list;
    while let Some(node) = cur {
        comp = match node.wrap {
            CompWrap::Grp(index) => mem.alloc(SerialComp::Grp(index, comp)),
            CompWrap::Seq(index) => mem.alloc(SerialComp::Seq(index, comp)),
        };
        cur = node.next;
    }
    comp
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
        let mut count = 0usize;
        let mut cur = serial;
        while let Serial::Next(_, _, rest) = cur {
            count += 1;
            cur = rest;
        }
        // DEEP Next nodes, then a Last, then Past.
        assert_eq!(count, DEEP);
        assert!(matches!(cur, Serial::Last(_, _)));
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
        let Serial::Last(term, _) = serial else {
            panic!("expected Last")
        };
        let mut count = 0usize;
        let mut cur: &SerialTerm = term;
        while let SerialTerm::Nest(inner) = cur {
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
        let Serial::Last(term, _) = serial else {
            panic!("expected Last")
        };
        // The outermost Pack is entered first and gets index 0; indices then
        // increase inward.
        let mut expected = 0u64;
        let mut cur: &SerialTerm = term;
        while let SerialTerm::Pack(index, inner) = cur {
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
        // Should not overflow; a single leaf yields a trivial Last serial.
        let serial = serialize(&mem, edsl);
        assert!(matches!(serial, Serial::Last(_, _)));
    }
}
