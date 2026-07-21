//! Shared spine walking for the `DenullDoc`-consuming passes.
//!
//! `identities`, `reassociate`, and `rescope` all walk the same linear
//! `DenullDoc` spine (`Eod`/`Empty`/`Break`/`Line`), transform each line's
//! object, and fold the results back onto the terminal. They differ only in the
//! per-object transform and the output spine type (`DenullDoc` for the first
//! two, `FinalDoc` for `rescope`). [`map_denull_spine`] captures that one shape;
//! [`SpineSink`] abstracts the output constructors.

use crate::compiler::types::{DenullDoc, DenullObj, FinalDoc, FinalDocObj};
use bumpalo::Bump;

/// An output spine a `DenullDoc` walk can fold into. Implemented for the pass
/// output types that share the `Eod`/`Empty`/`Break`/`Line` shape.
pub trait SpineSink<'b>: Sized {
    /// The per-line object this spine carries.
    type Obj;
    fn eod(mem: &'b Bump) -> &'b Self;
    fn empty(mem: &'b Bump, tail: &'b Self) -> &'b Self;
    fn brk(mem: &'b Bump, obj: Self::Obj, tail: &'b Self) -> &'b Self;
    fn line(mem: &'b Bump, obj: Self::Obj) -> &'b Self;
}

impl<'b> SpineSink<'b> for DenullDoc<'b> {
    type Obj = &'b DenullObj<'b>;
    fn eod(mem: &'b Bump) -> &'b Self {
        mem.alloc(DenullDoc::Eod)
    }
    fn empty(mem: &'b Bump, tail: &'b Self) -> &'b Self {
        mem.alloc(DenullDoc::Empty(tail))
    }
    fn brk(mem: &'b Bump, obj: Self::Obj, tail: &'b Self) -> &'b Self {
        mem.alloc(DenullDoc::Break(obj, tail))
    }
    fn line(mem: &'b Bump, obj: Self::Obj) -> &'b Self {
        mem.alloc(DenullDoc::Line(obj))
    }
}

impl<'b> SpineSink<'b> for FinalDoc<'b> {
    type Obj = &'b FinalDocObj<'b>;
    fn eod(mem: &'b Bump) -> &'b Self {
        mem.alloc(FinalDoc::Eod)
    }
    fn empty(mem: &'b Bump, tail: &'b Self) -> &'b Self {
        mem.alloc(FinalDoc::Empty(tail))
    }
    fn brk(mem: &'b Bump, obj: Self::Obj, tail: &'b Self) -> &'b Self {
        mem.alloc(FinalDoc::Break(obj, tail))
    }
    fn line(mem: &'b Bump, obj: Self::Obj) -> &'b Self {
        mem.alloc(FinalDoc::Line(obj))
    }
}

/// Walk a linear `DenullDoc` spine, mapping each line's object with `map_obj`,
/// and fold the results back onto the terminal as an output spine `S`.
///
/// Iterative: a plain loop down the spine collecting into a `Vec`, then a
/// reverse fold, so arbitrarily deep documents use no native stack. The output
/// spine type is inferred from the caller's return type.
pub fn map_denull_spine<'b, 'a: 'b, S: SpineSink<'b>>(
    mem: &'b Bump,
    doc: &'a DenullDoc<'a>,
    map_obj: impl Fn(&'b Bump, &'a DenullObj<'a>) -> S::Obj,
) -> &'b S {
    enum Item<O> {
        Empty,
        Break(O),
    }
    let mut items: Vec<Item<S::Obj>> = Vec::new();
    let mut cur = doc;
    let terminal: &'b S = loop {
        match cur {
            DenullDoc::Eod => break S::eod(mem),
            DenullDoc::Line(obj) => break S::line(mem, map_obj(mem, obj)),
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
            Item::Empty => S::empty(mem, result),
            Item::Break(obj) => S::brk(mem, obj, result),
        };
    }
    result
}
