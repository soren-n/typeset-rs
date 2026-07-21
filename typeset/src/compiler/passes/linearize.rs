//! Pass 3: Serial → LinearDoc (lift newlines to spine)
//!
//! Serial is a flat slice and Term/SerialComp are linear (single-child)
//! chains, so the original CPS recursion is replaced by plain iteration with
//! small wrapper stacks. This keeps the pass off the native stack, which the
//! recursive/continuation version could exhaust on deep inputs.

use super::term_chain::map_term_chain;
use crate::compiler::types::{
    LinearComp, LinearDoc, LinearObj, Serial, SerialComp, SerialEntry, Term,
};
use bumpalo::Bump;

pub fn linearize<'b, 'a: 'b>(mem: &'b Bump, serial: Serial<'a>) -> LinearDoc<'b> {
    // Completed line objects, in document order.
    let mut lines: Vec<&'b LinearObj<'b>> = Vec::new();
    // Elements of the line currently being built, in visitation order.
    let mut acc: Vec<(&'b Term<'b>, &'b LinearComp<'b>)> = Vec::new();

    for entry in serial {
        match entry {
            // A `Line` composition ends the current line: flush the accumulated
            // elements (with this entry's term as the final element) as one
            // object, then start a fresh line.
            SerialEntry::Next(term, SerialComp::Line) => {
                let term1 = map_term_chain(mem, *term);
                lines.push(build_line(mem, &acc, term1));
                acc.clear();
            }
            // Any other composition extends the current line.
            SerialEntry::Next(term, comp) => {
                let term1 = map_term_chain(mem, *term);
                let comp1 = visit_comp(mem, comp);
                acc.push((term1, comp1));
            }
            // The document's final term: flush the last line.
            SerialEntry::Last(term) => {
                let term1 = map_term_chain(mem, *term);
                lines.push(build_line(mem, &acc, term1));
            }
        }
    }

    // The completed lines are already in document order; the spine is a flat
    // slice with no sharing, so copy them straight into the arena.
    mem.alloc_slice_copy(&lines)
}

/// Builds one line object: `Next(t0, c0, Next(t1, c1, ... Last(term_last)))`,
/// where `acc` holds the `(term, comp)` pairs in visitation order and
/// `term_last` is the line's final term.
fn build_line<'b>(
    mem: &'b Bump,
    acc: &[(&'b Term<'b>, &'b LinearComp<'b>)],
    term_last: &'b Term<'b>,
) -> &'b LinearObj<'b> {
    let mut obj: &'b LinearObj<'b> = mem.alloc(LinearObj::Last(term_last));
    for (term, comp) in acc.iter().rev() {
        obj = mem.alloc(LinearObj::Next(term, comp, obj));
    }
    obj
}

/// Carries a non-`Line` `SerialComp` through to a `LinearComp`. The scope
/// delta slices are `Copy` and outlive this pass's arena, so they pass through
/// by borrow — no per-comp rebuild.
fn visit_comp<'b, 'a: 'b>(mem: &'b Bump, comp: &'a SerialComp<'a>) -> &'b LinearComp<'b> {
    match comp {
        SerialComp::Line => unreachable!("Invariant"),
        SerialComp::Comp(attr, opens, closes) => mem.alloc(LinearComp {
            attr: *attr,
            opens,
            closes,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{Attr, Term};

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    #[test]
    fn linearize_handles_long_serial_spine() {
        let mem = Bump::new();
        let attr = Attr {
            pad: false,
            fix: false,
        };
        // Build DEEP Comp entries followed by a final Last entry.
        let mut entries: Vec<SerialEntry> = Vec::new();
        for _ in 0..DEEP {
            entries.push(SerialEntry::Next(
                mem.alloc(Term::Text("x")),
                mem.alloc(SerialComp::Comp(attr, &[], &[])),
            ));
        }
        entries.push(SerialEntry::Last(mem.alloc(Term::Text("end"))));
        let serial: Serial = mem.alloc_slice_copy(&entries);
        let doc = linearize(&mem, serial);
        // One line (no Line comps), a Next-chain of DEEP + 1 terms.
        assert_eq!(doc.len(), 1, "expected one line");
        let obj = doc[0];
        let mut count = 0usize;
        let mut cur = obj;
        while let LinearObj::Next(_, _, next) = cur {
            count += 1;
            cur = next;
        }
        assert!(matches!(cur, LinearObj::Last(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn linearize_handles_deep_term_and_comp() {
        let mem = Bump::new();
        // Deep Nest term on a single element; the comp carries its scope
        // deltas by borrow (no per-comp recursion to overflow anymore).
        let mut term: &Term = mem.alloc(Term::Text("x"));
        for _ in 0..DEEP {
            term = mem.alloc(Term::Nest(term));
        }
        let comp: &SerialComp = mem.alloc(SerialComp::Comp(
            Attr {
                pad: false,
                fix: false,
            },
            &[],
            &[],
        ));
        let serial: Serial = mem.alloc_slice_copy(&[
            SerialEntry::Next(term, comp),
            SerialEntry::Last(mem.alloc(Term::Text("end"))),
        ]);
        let doc = linearize(&mem, serial);
        // Confirm the deep term nesting survived.
        let [obj, ..] = doc else {
            panic!("expected a line")
        };
        let LinearObj::Next(t, _, _) = obj else {
            panic!("expected Next")
        };
        let mut count = 0usize;
        let mut cur = *t;
        while let Term::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }
}
