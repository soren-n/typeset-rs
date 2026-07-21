//! Shared nest/pack term-chain mapping for the linear passes.
//!
//! `linearize`, `fixed`, `graphify`, and `rebuild` each rebuild a term into the
//! next representation's arena, and a term is always a linear chain of
//! `Nest`/`Pack` wrappers over a `Null`/`Text` leaf. Most passes keep the shared
//! [`Term`] type on both sides; graphify/rebuild convert between `Term` and
//! [`GraphTerm`]. That strip-and-rebuild is identical modulo the source and
//! destination enums. [`map_term_chain`] captures the one shape; [`TermChain`]
//! classifies each step of the input and [`TermSink`] abstracts the leaf/wrapper
//! constructors of the output.

use crate::compiler::types::{GraphTerm, Term};
use bumpalo::Bump;

/// One step down a term chain toward its leaf: either the `Null`/`Text` leaf,
/// or a `Nest`/`Pack` wrapper carrying the child to descend into.
pub enum TermStep<'a, T> {
    Null,
    Text(&'a str),
    Nest(&'a T),
    Pack(u64, &'a T),
}

/// An input term type shaped as a `Nest`/`Pack` chain over a `Null`/`Text`
/// leaf. `step` reports how the current node continues the chain.
pub trait TermChain<'a>: Sized {
    fn step(&'a self) -> TermStep<'a, Self>;
}

/// An output term type built from the four leaf/wrapper constructors.
pub trait TermSink<'b>: Sized {
    fn null(mem: &'b Bump) -> &'b Self;
    fn text(mem: &'b Bump, data: &'b str) -> &'b Self;
    fn nest(mem: &'b Bump, inner: &'b Self) -> &'b Self;
    fn pack(mem: &'b Bump, index: u64, inner: &'b Self) -> &'b Self;
}

/// Rebuilds a term chain `S` into the next representation `D`, preserving
/// nest/pack nesting.
///
/// Iterative: descend to the leaf recording each wrapper in a `Vec`, then
/// rebuild bottom-up, so arbitrarily deep terms use no native stack. The output
/// type is inferred from the caller's return type.
pub fn map_term_chain<'b, 'a: 'b, S: TermChain<'a>, D: TermSink<'b>>(
    mem: &'b Bump,
    term: &'a S,
) -> &'b D {
    enum Wrap {
        Nest,
        Pack(u64),
    }
    let mut wraps: Vec<Wrap> = Vec::new();
    let mut cur = term;
    let mut val: &'b D = loop {
        match cur.step() {
            TermStep::Null => break D::null(mem),
            TermStep::Text(data) => break D::text(mem, data),
            TermStep::Nest(term1) => {
                wraps.push(Wrap::Nest);
                cur = term1;
            }
            TermStep::Pack(index, term1) => {
                wraps.push(Wrap::Pack(index));
                cur = term1;
            }
        }
    };
    while let Some(wrap) = wraps.pop() {
        val = match wrap {
            Wrap::Nest => D::nest(mem, val),
            Wrap::Pack(index) => D::pack(mem, index, val),
        };
    }
    val
}

impl<'a> TermChain<'a> for Term<'a> {
    fn step(&'a self) -> TermStep<'a, Self> {
        match self {
            Term::Null => TermStep::Null,
            Term::Text(data) => TermStep::Text(data),
            Term::Nest(term1) => TermStep::Nest(term1),
            Term::Pack(index, term1) => TermStep::Pack(*index, term1),
        }
    }
}

impl<'a> TermChain<'a> for GraphTerm<'a> {
    fn step(&'a self) -> TermStep<'a, Self> {
        match self {
            GraphTerm::Null => TermStep::Null,
            GraphTerm::Text(data) => TermStep::Text(data),
            GraphTerm::Nest(term1) => TermStep::Nest(term1),
            GraphTerm::Pack(index, term1) => TermStep::Pack(*index, term1),
            GraphTerm::Fix(_fix) => unreachable!("Invariant"),
        }
    }
}

impl<'b> TermSink<'b> for Term<'b> {
    fn null(mem: &'b Bump) -> &'b Self {
        mem.alloc(Term::Null)
    }
    fn text(mem: &'b Bump, data: &'b str) -> &'b Self {
        mem.alloc(Term::Text(data))
    }
    fn nest(mem: &'b Bump, inner: &'b Self) -> &'b Self {
        mem.alloc(Term::Nest(inner))
    }
    fn pack(mem: &'b Bump, index: u64, inner: &'b Self) -> &'b Self {
        mem.alloc(Term::Pack(index, inner))
    }
}

impl<'b> TermSink<'b> for GraphTerm<'b> {
    fn null(mem: &'b Bump) -> &'b Self {
        mem.alloc(GraphTerm::Null)
    }
    fn text(mem: &'b Bump, data: &'b str) -> &'b Self {
        mem.alloc(GraphTerm::Text(data))
    }
    fn nest(mem: &'b Bump, inner: &'b Self) -> &'b Self {
        mem.alloc(GraphTerm::Nest(inner))
    }
    fn pack(mem: &'b Bump, index: u64, inner: &'b Self) -> &'b Self {
        mem.alloc(GraphTerm::Pack(index, inner))
    }
}
