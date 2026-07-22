//! Shared spine walking for the `normalize` pass.
//!
//! `normalize` walks the linear `DenullDoc` spine (`Eod`/`Empty`/`Break`/`Line`),
//! transforms each line's object (running the seq/grp elimination and
//! reassociation folds), and folds the results back onto the terminal as a fresh
//! `DenullDoc`. The per-object transform is passed to [`map_denull_spine`] as a
//! closure. (The final `rescope` pass walks the same spine but folds directly
//! into the heap [`Doc`], so it does its own spine walk rather than allocating a
//! `DenullDoc`.)

use crate::compiler::types::{DenullDoc, DenullObj};
use bumpalo::Bump;

/// Walk a linear `DenullDoc` spine, mapping each line's object with `map_obj`,
/// and fold the results back onto the terminal as a fresh `DenullDoc`.
///
/// Iterative: a plain loop down the spine collecting into a `Vec`, then a
/// reverse fold, so arbitrarily deep documents use no native stack.
pub fn map_denull_spine<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a DenullDoc<'a>,
    map_obj: impl Fn(&'b Bump, &'a DenullObj<'a>) -> &'b DenullObj<'b>,
) -> &'b DenullDoc<'b> {
    enum Item<'b> {
        Empty,
        Break(&'b DenullObj<'b>),
    }
    let mut items: Vec<Item<'b>> = Vec::new();
    let mut cur = doc;
    let terminal: &'b DenullDoc<'b> = loop {
        match cur {
            DenullDoc::Eod => break mem.alloc(DenullDoc::Eod),
            DenullDoc::Line(obj) => break mem.alloc(DenullDoc::Line(map_obj(mem, obj))),
            DenullDoc::Empty(doc1) => {
                items.push(Item::Empty);
                cur = doc1;
            }
            DenullDoc::Break(obj, doc1) => {
                items.push(Item::Break(map_obj(mem, obj)));
                cur = doc1;
            }
        }
    };
    let mut result = terminal;
    for item in items.into_iter().rev() {
        result = match item {
            Item::Empty => mem.alloc(DenullDoc::Empty(result)),
            Item::Break(obj) => mem.alloc(DenullDoc::Break(obj, result)),
        };
    }
    result
}
