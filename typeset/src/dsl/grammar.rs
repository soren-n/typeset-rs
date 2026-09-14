//! The grammar with one implementation: the tokens, the token parser, and
//! the builder a front end supplies. A contract between this crate and the
//! `layout!` macro, published in lockstep; not an API.

use super::ParseError;
use crate::layout::{Break, Pad};

/// A prefix operator.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Unary {
    Fix,
    Grp,
    Seq,
    Nest,
    Pack,
}

impl Unary {
    /// The keyword.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            Unary::Fix => "fix",
            Unary::Grp => "grp",
            Unary::Seq => "seq",
            Unary::Nest => "nest",
            Unary::Pack => "pack",
        }
    }

    /// The keyword for `name`, if it is one.
    #[must_use]
    pub fn from_keyword(name: &str) -> Option<Unary> {
        Some(match name {
            "fix" => Unary::Fix,
            "grp" => Unary::Grp,
            "seq" => Unary::Seq,
            "nest" => Unary::Nest,
            "pack" => Unary::Pack,
            _ => return None,
        })
    }
}

/// An infix operator.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Binary {
    /// `@`
    Line,
    /// `@@`
    BlankLine,
    /// `&`, `+`, `!&`, `!+`
    Comp(Pad, Break),
}

impl Binary {
    /// The operator's spelling.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Binary::Line => "@",
            Binary::BlankLine => "@@",
            Binary::Comp(Pad::Unpadded, Break::Breakable) => "&",
            Binary::Comp(Pad::Padded, Break::Breakable) => "+",
            Binary::Comp(Pad::Unpadded, Break::Fixed) => "!&",
            Binary::Comp(Pad::Padded, Break::Fixed) => "!+",
        }
    }

    /// The operator spelled `op`, if any. Longest match is the caller's job:
    /// `!&` and `@@` must arrive whole.
    #[must_use]
    pub fn from_symbol(op: &str) -> Option<Binary> {
        Some(match op {
            "@" => Binary::Line,
            "@@" => Binary::BlankLine,
            "&" => Binary::Comp(Pad::Unpadded, Break::Breakable),
            "+" => Binary::Comp(Pad::Padded, Break::Breakable),
            "!&" => Binary::Comp(Pad::Unpadded, Break::Fixed),
            "!+" => Binary::Comp(Pad::Padded, Break::Fixed),
            _ => return None,
        })
    }
}

/// A token of the DSL. `V` is the front end's variable payload (a Rust
/// identifier in the macro; uninhabited at run time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token<V> {
    Open,
    Close,
    Null,
    Unary(Unary),
    Text(String),
    Var(V),
    Binary(Binary),
}

/// What a front end builds the parsed layout into: a `Layout` at run time,
/// constructor calls in the macro. One method per constructor; the parser
/// desugars `@@` into two lines around a null.
pub trait Build {
    /// The variable payload of the front end's [`Token::Var`].
    type Var;
    type Out;
    fn null(&mut self) -> Self::Out;
    fn text(&mut self, data: String) -> Self::Out;
    fn var(&mut self, var: Self::Var) -> Self::Out;
    fn unary(&mut self, op: Unary, layout: Self::Out) -> Self::Out;
    fn line(&mut self, left: Self::Out, right: Self::Out) -> Self::Out;
    fn comp(&mut self, left: Self::Out, right: Self::Out, pad: Pad, brk: Break) -> Self::Out;
}

/// Parses `tokens` (each with its position) with `builder`. `end` is the
/// position reported for input that ends inside an expression.
pub fn parse_tokens<P, B: Build>(
    tokens: impl IntoIterator<Item = (P, Token<B::Var>)>,
    end: P,
    builder: &mut B,
) -> Result<B::Out, ParseError<P>> {
    let mut frames = vec![Frame::new()];
    // Whether the next token must be an operand (else an operator or `)`).
    let mut want_operand = true;
    let err = |at, message| ParseError { at, message };
    for (at, token) in tokens {
        let frame = frames.last_mut().expect("the root frame is never popped");
        if want_operand {
            match token {
                Token::Null => {
                    let out = builder.null();
                    frame.push_operand(builder, out);
                }
                Token::Text(data) => {
                    let out = builder.text(data);
                    frame.push_operand(builder, out);
                }
                Token::Var(var) => {
                    let out = builder.var(var);
                    frame.push_operand(builder, out);
                }
                Token::Unary(unary) => {
                    if frame.unary.is_some() {
                        return Err(err(at, "expected a primary expression"));
                    }
                    frame.unary = Some(unary);
                    continue;
                }
                Token::Open => {
                    frames.push(Frame::new());
                    continue;
                }
                Token::Close | Token::Binary(_) => {
                    return Err(err(at, "expected a primary expression"));
                }
            }
            want_operand = false;
        } else {
            match token {
                Token::Binary(op) => {
                    frame.ops.push(op);
                    want_operand = true;
                }
                Token::Close => {
                    if frames.len() == 1 {
                        return Err(err(at, "unexpected )"));
                    }
                    let inner = frames.pop().expect("nested frame").finish(builder);
                    frames
                        .last_mut()
                        .expect("root frame")
                        .push_operand(builder, inner);
                }
                _ => return Err(err(at, "expected an operator")),
            }
        }
    }
    if want_operand {
        return Err(err(end, "expected a primary expression"));
    }
    if frames.len() != 1 {
        return Err(err(end, "expected )"));
    }
    Ok(frames.pop().expect("root frame").finish(builder))
}

/// One parenthesized level: the operand chain built so far, the operators
/// between the operands, and a unary operator waiting for its operand.
struct Frame<O> {
    operands: Vec<O>,
    ops: Vec<Binary>,
    unary: Option<Unary>,
}

impl<O> Frame<O> {
    fn new() -> Self {
        Frame {
            operands: Vec::new(),
            ops: Vec::new(),
            unary: None,
        }
    }

    fn push_operand<B: Build<Out = O>>(&mut self, builder: &mut B, operand: O) {
        let operand = match self.unary.take() {
            Some(unary) => builder.unary(unary, operand),
            None => operand,
        };
        self.operands.push(operand);
    }

    /// Folds the chain right-associatively: `a op1 b op2 c` is
    /// `a op1 (b op2 c)`.
    fn finish<B: Build<Out = O>>(mut self, builder: &mut B) -> O {
        let mut result = self
            .operands
            .pop()
            .expect("a finished frame has an operand");
        while let (Some(op), Some(left)) = (self.ops.pop(), self.operands.pop()) {
            result = match op {
                Binary::Line => builder.line(left, result),
                Binary::BlankLine => {
                    let blank = builder.null();
                    let tail = builder.line(blank, result);
                    builder.line(left, tail)
                }
                Binary::Comp(pad, brk) => builder.comp(left, result, pad, brk),
            };
        }
        result
    }
}

/// Reads one string literal, quotes included, into its text. This is the
/// DSL's string syntax: the `layout!` macro feeds it the source text of its
/// Rust string literals, so both front ends accept exactly the same
/// literals (and raw strings or Rust-only escapes are rejected).
pub fn parse_text(literal: &str) -> Result<String, ParseError> {
    let (text, end) = super::scan_text(literal, 0)?;
    if end != literal.len() {
        return Err(ParseError {
            at: end,
            message: "expected the end of the literal",
        });
    }
    Ok(text)
}
