//! Pass 3: Serial → LinearDoc (lift newlines to spine)
//!
//! Serial, SerialTerm and SerialComp are all linear (single-child) chains, so
//! the original CPS recursion is replaced by plain iteration with small
//! wrapper stacks. This keeps the pass off the native stack, which the
//! recursive/continuation version could exhaust on deep inputs.

use crate::compiler::types::{
    LinearComp, LinearDoc, LinearObj, LinearTerm, Serial, SerialComp, SerialTerm,
};
use bumpalo::Bump;

/// A `SerialTerm` wrapper, recorded outermost-first while descending.
enum TermWrap {
    Nest,
    Pack(u64),
}

/// A `SerialComp` wrapper, recorded outermost-first while descending.
enum CompWrap {
    Grp(u64),
    Seq(u64),
}

pub fn linearize<'b, 'a: 'b>(mem: &'b Bump, serial: &'a Serial<'a>) -> &'b LinearDoc<'b> {
    // Completed line objects, in document order.
    let mut lines: Vec<&'b LinearObj<'b>> = Vec::new();
    // Elements of the line currently being built, in visitation order.
    let mut acc: Vec<(&'b LinearTerm<'b>, &'b LinearComp<'b>)> = Vec::new();

    let mut cur = serial;
    loop {
        match cur {
            // A `Line` composition ends the current line: flush the accumulated
            // elements (with this node's term as the final element) as one
            // object, then start a fresh line.
            Serial::Next(term, SerialComp::Line, serial1) => {
                let term1 = _visit_term(mem, term);
                lines.push(_build_line(mem, &acc, term1));
                acc.clear();
                cur = serial1;
            }
            // Any other composition extends the current line.
            Serial::Next(term, comp, serial1) => {
                let term1 = _visit_term(mem, term);
                let comp1 = _visit_comp(mem, comp);
                acc.push((term1, comp1));
                cur = serial1;
            }
            // End of the serial: flush the final line.
            Serial::Last(term, Serial::Past) => {
                let term1 = _visit_term(mem, term);
                lines.push(_build_line(mem, &acc, term1));
                break;
            }
            _ => unreachable!("Invariant"),
        }
    }

    // Fold the completed lines (document order) into a Cons list ending in Nil.
    let mut doc: &'b LinearDoc<'b> = mem.alloc(LinearDoc::Nil);
    for obj in lines.iter().rev() {
        doc = mem.alloc(LinearDoc::Cons(obj, doc));
    }
    doc
}

/// Builds one line object: `Next(t0, c0, Next(t1, c1, ... Last(term_last)))`,
/// where `acc` holds the `(term, comp)` pairs in visitation order and
/// `term_last` is the line's final term.
fn _build_line<'b>(
    mem: &'b Bump,
    acc: &[(&'b LinearTerm<'b>, &'b LinearComp<'b>)],
    term_last: &'b LinearTerm<'b>,
) -> &'b LinearObj<'b> {
    let mut obj: &'b LinearObj<'b> = mem.alloc(LinearObj::Last(term_last));
    for (term, comp) in acc.iter().rev() {
        obj = mem.alloc(LinearObj::Next(term, comp, obj));
    }
    obj
}

/// Linearizes a `SerialTerm` chain into a `LinearTerm`, preserving nesting.
fn _visit_term<'b, 'a: 'b>(mem: &'b Bump, term: &'a SerialTerm<'a>) -> &'b LinearTerm<'b> {
    let mut wraps: Vec<TermWrap> = Vec::new();
    let mut cur = term;
    let mut val: &'b LinearTerm<'b> = loop {
        match cur {
            SerialTerm::Null => break mem.alloc(LinearTerm::Null),
            SerialTerm::Text(data) => break mem.alloc(LinearTerm::Text(data)),
            SerialTerm::Nest(term1) => {
                wraps.push(TermWrap::Nest);
                cur = term1;
            }
            SerialTerm::Pack(index, term1) => {
                wraps.push(TermWrap::Pack(*index));
                cur = term1;
            }
        }
    };
    while let Some(wrap) = wraps.pop() {
        val = match wrap {
            TermWrap::Nest => mem.alloc(LinearTerm::Nest(val)),
            TermWrap::Pack(index) => mem.alloc(LinearTerm::Pack(index, val)),
        };
    }
    val
}

/// Linearizes a non-`Line` `SerialComp` chain into a `LinearComp`.
fn _visit_comp<'b, 'a: 'b>(mem: &'b Bump, comp: &'a SerialComp<'a>) -> &'b LinearComp<'b> {
    let mut wraps: Vec<CompWrap> = Vec::new();
    let mut cur = comp;
    let mut val: &'b LinearComp<'b> = loop {
        match cur {
            SerialComp::Line => unreachable!("Invariant"),
            SerialComp::Comp(attr) => break mem.alloc(LinearComp::Comp(*attr)),
            SerialComp::Grp(index, comp1) => {
                wraps.push(CompWrap::Grp(*index));
                cur = comp1;
            }
            SerialComp::Seq(index, comp1) => {
                wraps.push(CompWrap::Seq(*index));
                cur = comp1;
            }
        }
    };
    while let Some(wrap) = wraps.pop() {
        val = match wrap {
            CompWrap::Grp(index) => mem.alloc(LinearComp::Grp(index, val)),
            CompWrap::Seq(index) => mem.alloc(LinearComp::Seq(index, val)),
        };
    }
    val
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::Attr;

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
        // Build Next(Text, Comp, Next(...)) ending in Last(Text, Past).
        let mut serial: &Serial = mem.alloc(Serial::Last(
            mem.alloc(SerialTerm::Text("end")),
            mem.alloc(Serial::Past),
        ));
        for _ in 0..DEEP {
            serial = mem.alloc(Serial::Next(
                mem.alloc(SerialTerm::Text("x")),
                mem.alloc(SerialComp::Comp(attr)),
                serial,
            ));
        }
        let doc = linearize(&mem, serial);
        // One line (no Line comps), a Next-chain of DEEP + 1 terms.
        let obj = match doc {
            LinearDoc::Cons(obj, rest) => {
                assert!(matches!(rest, LinearDoc::Nil));
                *obj
            }
            LinearDoc::Nil => panic!("expected one line"),
        };
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
        // Deep Nest term and deep Grp comp on a single element.
        let mut term: &SerialTerm = mem.alloc(SerialTerm::Text("x"));
        for _ in 0..DEEP {
            term = mem.alloc(SerialTerm::Nest(term));
        }
        let mut comp: &SerialComp = mem.alloc(SerialComp::Comp(Attr {
            pad: false,
            fix: false,
        }));
        for _ in 0..DEEP {
            comp = mem.alloc(SerialComp::Grp(0, comp));
        }
        let serial: &Serial = mem.alloc(Serial::Next(
            term,
            comp,
            mem.alloc(Serial::Last(
                mem.alloc(SerialTerm::Text("end")),
                mem.alloc(Serial::Past),
            )),
        ));
        let doc = linearize(&mem, serial);
        // Confirm the deep term nesting survived.
        let LinearDoc::Cons(obj, _) = doc else {
            panic!("expected a line")
        };
        let LinearObj::Next(t, _, _) = obj else {
            panic!("expected Next")
        };
        let mut count = 0usize;
        let mut cur = *t;
        while let LinearTerm::Nest(inner) = cur {
            count += 1;
            cur = inner;
        }
        assert_eq!(count, DEEP);
    }
}
