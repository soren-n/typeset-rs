//! split_lines: SerialDoc entries → FixedDoc (lift newlines to spine, coalesce fixed comps)
//!
//! One sweep over the serial entries: a hard line break flushes the current
//! line, and maximal runs of terms joined by fixed compositions coalesce into
//! single fix items as each line is built. (Formerly two passes — `linearize`
//! split the lines and `fixed` coalesced the runs — with a cons-list IR
//! between them; the split and the coalescing are one forward scan.)

use crate::compiler::types::{
    FixRun, FixedComp, FixedDoc, FixedItem, FixedLine, FixedSpan, SerialComp, SerialEntry, Term,
};

/// Accumulates the flattened document. Items, line separators, run terms, and
/// run separators are appended straight into the shared arenas; the line and
/// fix run currently being built are tracked as start offsets, so a line or run
/// costs no allocation of its own — only the amortized growth of the arenas.
#[derive(Default)]
struct LineAccum<'a> {
    lines: Vec<FixedLine>,
    items: Vec<FixedItem<'a>>,
    item_seps: Vec<FixedComp>,
    terms: Vec<Term<'a>>,
    run_seps: Vec<FixedComp>,
    // Start offsets of the line currently being built.
    line_items_start: u32,
    line_seps_start: u32,
    // Start offsets of the fix run currently being coalesced; `run_open` is
    // false when no run is being built.
    run_terms_start: u32,
    run_seps_start: u32,
    run_open: bool,
}

impl<'a> LineAccum<'a> {
    /// Extends (or starts) the open fix run with `term` and the fixed
    /// composition `comp` that follows it.
    fn push_fixed(&mut self, term: Term<'a>, comp: FixedComp) {
        if !self.run_open {
            self.run_terms_start = self.terms.len() as u32;
            self.run_seps_start = self.run_seps.len() as u32;
            self.run_open = true;
        }
        self.terms.push(term);
        self.run_seps.push(comp);
    }

    /// Appends `term` as the line's next item: as the final term of the open
    /// fix run if one is being built, else as a plain term.
    fn push_item(&mut self, term: Term<'a>) {
        if !self.run_open {
            self.items.push(FixedItem::Term(term));
            return;
        }
        self.terms.push(term);
        self.items.push(FixedItem::Fix(FixRun {
            terms: FixedSpan {
                start: self.run_terms_start,
                end: self.terms.len() as u32,
            },
            seps: FixedSpan {
                start: self.run_seps_start,
                end: self.run_seps.len() as u32,
            },
        }));
        self.run_open = false;
    }

    /// Ends the current line with `term` as its last item.
    fn flush_line(&mut self, term: Term<'a>) {
        self.push_item(term);
        self.lines.push(FixedLine {
            items: FixedSpan {
                start: self.line_items_start,
                end: self.items.len() as u32,
            },
            seps: FixedSpan {
                start: self.line_seps_start,
                end: self.item_seps.len() as u32,
            },
        });
        self.line_items_start = self.items.len() as u32;
        self.line_seps_start = self.item_seps.len() as u32;
    }
}

pub fn split_lines<'a>(entries: &[SerialEntry<'a>]) -> FixedDoc<'a> {
    let mut acc = LineAccum::default();
    for entry in entries {
        match entry {
            // A hard line break ends the current line, as does the document's
            // final term: either way this entry's term is the line's last item.
            SerialEntry::Next(term, SerialComp::Line) | SerialEntry::Last(term) => {
                acc.flush_line(*term);
            }
            SerialEntry::Next(term, SerialComp::Comp(attr, opens, closes)) => {
                let comp = FixedComp {
                    pad: attr.pad.is_padded(),
                    opens: *opens,
                    closes: *closes,
                };
                if attr.brk.is_fixed() {
                    // A fixed composition: extend (or start) the current run.
                    acc.push_fixed(*term, comp);
                } else {
                    // A non-fixed composition separates items (closing the
                    // open run, if any).
                    acc.push_item(*term);
                    acc.item_seps.push(comp);
                }
            }
        }
    }
    FixedDoc {
        lines: acc.lines,
        items: acc.items,
        item_seps: acc.item_seps,
        terms: acc.terms,
        run_seps: acc.run_seps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{Attr, Break, NO_PATH, Pad, ScopeRange, TermLeaf};

    /// Far past where a native-stack recursion could survive; the pass is a
    /// plain scan, so this now guards sizing behavior only.
    const DEEP: usize = 50_000;

    fn text_term(text: &str) -> Term<'_> {
        Term {
            path: NO_PATH,
            leaf: TermLeaf::Text(text),
        }
    }

    const NO_SCOPES: ScopeRange = ScopeRange { start: 0, end: 0 };

    fn comp_entry(text: &str, fix: bool) -> SerialEntry<'_> {
        SerialEntry::Next(
            text_term(text),
            SerialComp::Comp(
                Attr {
                    pad: Pad::Unpadded,
                    brk: if fix { Break::Fixed } else { Break::Breakable },
                },
                NO_SCOPES,
                NO_SCOPES,
            ),
        )
    }

    #[test]
    fn split_lines_coalesces_deep_fixed_run() {
        // All comps fixed: the whole line collapses to one Fix item.
        let mut entries: Vec<SerialEntry> = Vec::new();
        for _ in 0..DEEP {
            entries.push(comp_entry("y", true));
        }
        entries.push(SerialEntry::Last(text_term("z")));
        let out = split_lines(&entries);
        let [line] = &out.lines[..] else {
            panic!("expected a single line")
        };
        let [FixedItem::Fix(run)] = line.items.slice(&out.items) else {
            panic!("expected a single Fix item")
        };
        assert_eq!(run.terms.slice(&out.terms).len(), DEEP + 1);
        assert_eq!(run.seps.slice(&out.run_seps).len(), DEEP);
    }

    #[test]
    fn split_lines_keeps_nonfixed_comps_as_separators() {
        // No fixed comps: DEEP + 1 plain term items with DEEP separators.
        let mut entries: Vec<SerialEntry> = Vec::new();
        for _ in 0..DEEP {
            entries.push(comp_entry("y", false));
        }
        entries.push(SerialEntry::Last(text_term("z")));
        let out = split_lines(&entries);
        let [line] = &out.lines[..] else {
            panic!("expected a single line")
        };
        let items = line.items.slice(&out.items);
        assert_eq!(items.len(), DEEP + 1);
        assert_eq!(line.seps.slice(&out.item_seps).len(), DEEP);
        assert!(items.iter().all(|i| matches!(i, FixedItem::Term(_))));
    }

    #[test]
    fn split_lines_splits_lines_at_hard_breaks() {
        let mut entries: Vec<SerialEntry> = Vec::new();
        for _ in 0..DEEP {
            entries.push(SerialEntry::Next(text_term("x"), SerialComp::Line));
        }
        entries.push(SerialEntry::Last(text_term("end")));
        let out = split_lines(&entries);
        assert_eq!(out.lines.len(), DEEP + 1);
    }

    #[test]
    fn split_lines_closes_run_at_nonfixed_comp() {
        // a !& b & c: the fixed run (a, b) closes at the non-fixed comp, which
        // becomes the separator before the plain term c.
        let entries = vec![
            comp_entry("a", true),
            comp_entry("b", false),
            SerialEntry::Last(text_term("c")),
        ];
        let out = split_lines(&entries);
        let [line] = &out.lines[..] else {
            panic!("expected a single line")
        };
        let [FixedItem::Fix(run), FixedItem::Term(_)] = line.items.slice(&out.items) else {
            panic!("expected a fix run then a plain term")
        };
        assert_eq!(run.terms.slice(&out.terms).len(), 2);
        assert_eq!(line.seps.slice(&out.item_seps).len(), 1);
    }
}
