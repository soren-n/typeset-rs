//! The layout DSL.
//!
//! One grammar, two front ends: [`parse`] reads it from a string at run
//! time, and the `typeset-parser` crate's `layout!` macro reads it from Rust
//! tokens at compile time by feeding the same token parser (the hidden
//! items of this module, a contract between the two crates).
//!
//! ```text
//! expr    := atom (binop expr)?          binops share one level, right-assoc
//! atom    := (fix | grp | seq | nest | pack)? primary
//! primary := null | "text" | variable | ( expr )
//! binop   := & | + | !& | !+ | @ | @@
//! ```
//!
//! `&`/`+` are unpadded/padded breakable compositions, `!&`/`!+` their fixed
//! forms, `@` a hard line break and `@@` a blank line. String literals accept
//! the escapes `\n \r \t \0 \\ \" \'`. Variables (bare identifiers standing
//! for a layout in scope) exist only in the macro.
//!
//! ```rust
//! use typeset::dsl;
//!
//! let layout = dsl::parse(r#"grp ("a" + "b") @ nest ("c" & "d")"#)?;
//! assert_eq!(layout.compile().render(2, 80), "a b\n  cd");
//! # Ok::<(), dsl::ParseError>(())
//! ```
//!
//! The parser is iterative: parenthesis depth costs heap, never native stack.

use crate::constructors::{comp, fix, grp, line, nest, null, pack, seq, text};
use crate::layout::{Break, Layout, Pad};
use std::fmt;

/// A prefix operator.
#[doc(hidden)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Unary {
    Fix,
    Grp,
    Seq,
    Nest,
    Pack,
}

impl Unary {
    /// The keyword for `name`, if it is one.
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
#[doc(hidden)]
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
    /// The operator spelled `op`, if any. Longest match is the caller's job:
    /// `!&` and `@@` must arrive whole.
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
#[doc(hidden)]
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

/// What a front end builds the parsed layout into: a [`Layout`] at run time,
/// constructor calls in the macro.
#[doc(hidden)]
pub trait Build {
    /// The variable payload of the front end's [`Token::Var`].
    type Var;
    type Out;
    fn null(&mut self) -> Self::Out;
    fn text(&mut self, data: String) -> Self::Out;
    fn var(&mut self, var: Self::Var) -> Self::Out;
    fn unary(&mut self, op: Unary, layout: Self::Out) -> Self::Out;
    fn binary(&mut self, op: Binary, left: Self::Out, right: Self::Out) -> Self::Out;
}

/// Why a token sequence failed to parse, and where: the position of the
/// offending token, or `end` for a sequence that ends too soon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseErrorAt<P> {
    pub at: P,
    pub message: &'static str,
}

/// Parses `tokens` (each with its position) with `builder`. `end` is the
/// position reported for input that ends inside an expression.
#[doc(hidden)]
pub fn parse_tokens<P, B: Build>(
    tokens: impl IntoIterator<Item = (P, Token<B::Var>)>,
    end: P,
    builder: &mut B,
) -> Result<B::Out, ParseErrorAt<P>> {
    let mut frames = vec![Frame::new()];
    // Whether the next token must be an operand (else an operator or `)`).
    let mut want_operand = true;
    let err = |at, message| ParseErrorAt { at, message };
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
            result = builder.binary(op, left, result);
        }
        result
    }
}

// --- The run-time front end ------------------------------------------------

/// Why a DSL string failed to parse (`message`), and the byte offset into
/// the source where it was detected (`at`).
pub type ParseError = ParseErrorAt<usize>;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.at)
    }
}

impl std::error::Error for ParseError {}

/// The run-time front end has no variables.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoVar {}

struct Constructors;

impl Build for Constructors {
    type Var = NoVar;
    type Out = Layout;
    fn null(&mut self) -> Layout {
        null()
    }
    fn text(&mut self, data: String) -> Layout {
        text(data)
    }
    fn var(&mut self, var: NoVar) -> Layout {
        match var {}
    }
    fn unary(&mut self, op: Unary, layout: Layout) -> Layout {
        match op {
            Unary::Fix => fix(layout),
            Unary::Grp => grp(layout),
            Unary::Seq => seq(layout),
            Unary::Nest => nest(layout),
            Unary::Pack => pack(layout),
        }
    }
    fn binary(&mut self, op: Binary, left: Layout, right: Layout) -> Layout {
        match op {
            Binary::Line => line(left, right),
            Binary::BlankLine => line(left, line(null(), right)),
            Binary::Comp(pad, brk) => comp(left, right, pad, brk),
        }
    }
}

/// Parses a layout from its DSL form.
pub fn parse(src: &str) -> Result<Layout, ParseError> {
    parse_tokens(tokenize(src)?, src.len(), &mut Constructors)
}

fn tokenize(src: &str) -> Result<Vec<(usize, Token<NoVar>)>, ParseError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let err = |message| ParseErrorAt { at: start, message };
        let token = match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => {
                i += 1;
                continue;
            }
            b'(' => {
                i += 1;
                Token::Open
            }
            b')' => {
                i += 1;
                Token::Close
            }
            b'"' => {
                let (text, end) = scan_text(src, i)?;
                i = end;
                Token::Text(text)
            }
            b'a'..=b'z' => {
                while i < bytes.len() && bytes[i].is_ascii_lowercase() {
                    i += 1;
                }
                match &src[start..i] {
                    "null" => Token::Null,
                    word => match Unary::from_keyword(word) {
                        Some(unary) => Token::Unary(unary),
                        None => return Err(err("unknown keyword")),
                    },
                }
            }
            _ => {
                // Longest match first, so `!&` and `@@` are not split.
                let two = src.get(i..i + 2).and_then(Binary::from_symbol);
                let one = src.get(i..i + 1).and_then(Binary::from_symbol);
                let (len, op) = match (two, one) {
                    (Some(op), _) => (2, op),
                    (None, Some(op)) => (1, op),
                    (None, None) => return Err(err("unexpected character")),
                };
                i += len;
                Token::Binary(op)
            }
        };
        tokens.push((start, token));
    }
    Ok(tokens)
}

/// Reads one string literal, quotes included, into its text. This is the
/// DSL's string syntax: the `layout!` macro feeds it the source text of its
/// Rust string literals, so both front ends accept exactly the same
/// literals (and raw strings or Rust-only escapes are rejected).
#[doc(hidden)]
pub fn parse_text(literal: &str) -> Result<String, ParseError> {
    let (text, end) = scan_text(literal, 0)?;
    if end != literal.len() {
        return Err(ParseErrorAt {
            at: end,
            message: "expected the end of the literal",
        });
    }
    Ok(text)
}

/// Scans the string literal starting at `start` in `src`, returning its
/// text and the offset just past its closing quote.
fn scan_text(src: &str, start: usize) -> Result<(String, usize), ParseError> {
    let bytes = src.as_bytes();
    let err = |message| ParseErrorAt { at: start, message };
    if bytes.get(start) != Some(&b'"') {
        return Err(err("expected a string literal"));
    }
    let mut data = String::new();
    let mut i = start + 1;
    loop {
        let Some(&c) = bytes.get(i) else {
            return Err(err("unterminated string"));
        };
        i += 1;
        match c {
            b'"' => return Ok((data, i)),
            b'\\' => {
                let Some(&e) = bytes.get(i) else {
                    return Err(err("dangling escape"));
                };
                i += 1;
                data.push(match e {
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'0' => '\0',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'\'' => '\'',
                    _ => return Err(err("unknown escape")),
                });
            }
            _ => {
                // Copy the whole UTF-8 sequence starting here.
                let ch = src[i - 1..].chars().next().expect("in bounds");
                data.push(ch);
                i += ch.len_utf8() - 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str, tab: usize, width: usize) -> String {
        parse(src).expect("parses").compile().render(tab, width)
    }

    #[test]
    fn operators_and_wrappers() {
        assert_eq!(fmt(r#""a" & "b""#, 2, 80), "ab");
        assert_eq!(fmt(r#""a" + "b""#, 2, 80), "a b");
        assert_eq!(fmt(r#""a" @ "b""#, 2, 80), "a\nb");
        assert_eq!(fmt(r#""a" @@ "b""#, 2, 80), "a\n\nb");
        assert_eq!(fmt(r#""a" !+ "b""#, 2, 1), "a b");
        assert_eq!(fmt(r#"grp (seq (("a" + "b") + "c"))"#, 2, 3), "a\nb\nc");
        assert_eq!(fmt(r#"nest ("a" @ "b")"#, 2, 80), "  a\n  b");
        assert_eq!(fmt(r#""x" + pack ("a" & ("b" @ "c"))"#, 2, 80), "x ab\n  c");
        assert_eq!(fmt("null", 2, 80), "");
        assert_eq!(fmt(r#"fix ("a" + "b")"#, 2, 1), "a b");
    }

    #[test]
    fn chains_associate_right() {
        // `"a" + "b" @ "c"` is `"a" + ("b" @ "c")`: the line break is inside
        // the composition's right operand.
        assert_eq!(fmt(r#""a" + "b" @ "c""#, 2, 80), "a b\nc");
    }

    #[test]
    fn unary_applies_to_the_next_primary_only() {
        assert_eq!(fmt(r#"nest "a" @ "b""#, 2, 80), "  a\nb");
        assert_eq!(fmt(r#"nest ("a" @ "b")"#, 2, 80), "  a\n  b");
    }

    #[test]
    fn string_escapes_and_unicode() {
        assert_eq!(fmt(r#""a\"b\\c\n""#, 2, 80), "a\"b\\c\n");
        assert_eq!(fmt(r#""héllo" & "→""#, 2, 80), "héllo→");
    }

    #[test]
    fn errors_carry_offsets() {
        let e = parse(r#""a" + "#).unwrap_err();
        assert_eq!((e.at, e.message), (6, "expected a primary expression"));
        let e = parse(r#"("a""#).unwrap_err();
        assert_eq!((e.at, e.message), (4, "expected )"));
        let e = parse(r#""a")"#).unwrap_err();
        assert_eq!((e.at, e.message), (3, "unexpected )"));
        let e = parse(r#""a" "b""#).unwrap_err();
        assert_eq!((e.at, e.message), (4, "expected an operator"));
        let e = parse(r#"grp grp ("a")"#).unwrap_err();
        assert_eq!((e.at, e.message), (4, "expected a primary expression"));
        let e = parse(r#""abc"#).unwrap_err();
        assert_eq!((e.at, e.message), (0, "unterminated string"));
        let e = parse("foo").unwrap_err();
        assert_eq!((e.at, e.message), (0, "unknown keyword"));
        let e = parse("\"a\" ! \"b\"").unwrap_err();
        assert_eq!((e.at, e.message), (4, "unexpected character"));
        assert_eq!(
            parse("").unwrap_err().message,
            "expected a primary expression"
        );
    }

    #[test]
    fn text_literals_are_read_whole() {
        assert_eq!(parse_text(r#""a\"b""#).unwrap(), "a\"b");
        let e = parse_text(r#"r"a""#).unwrap_err();
        assert_eq!((e.at, e.message), (0, "expected a string literal"));
        let e = parse_text(r#""a"b"#).unwrap_err();
        assert_eq!((e.at, e.message), (3, "expected the end of the literal"));
        let e = parse_text(r#""\u{e9}""#).unwrap_err();
        assert_eq!((e.at, e.message), (0, "unknown escape"));
    }
}
