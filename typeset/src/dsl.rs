//! A runtime parser for the layout DSL.
//!
//! The same language the `typeset-parser` crate's `layout!` macro accepts at
//! compile time, parsed from a string at run time:
//!
//! ```text
//! expr    := atom (binop expr)?          binops share one level, right-assoc
//! atom    := (fix | grp | seq | nest | pack)? primary
//! primary := null | "text" | ( expr )
//! binop   := & | + | !& | !+ | @ | @@
//! ```
//!
//! `&`/`+` are unpadded/padded breakable compositions, `!&`/`!+` their fixed
//! forms, `@` a hard line break and `@@` a blank line. String literals accept
//! the escapes `\n \r \t \0 \\ \" \'`.
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

/// Why a DSL string failed to parse, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    offset: usize,
    message: &'static str,
}

impl ParseError {
    /// Byte offset into the source where the error was detected.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// What was wrong.
    pub fn message(&self) -> &'static str {
        self.message
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Copy, Clone)]
enum Unary {
    Fix,
    Grp,
    Seq,
    Nest,
    Pack,
}

impl Unary {
    fn apply(self, layout: Layout) -> Layout {
        match self {
            Unary::Fix => fix(layout),
            Unary::Grp => grp(layout),
            Unary::Seq => seq(layout),
            Unary::Nest => nest(layout),
            Unary::Pack => pack(layout),
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum Binary {
    Line,
    BlankLine,
    Comp(Pad, Break),
}

impl Binary {
    fn apply(self, left: Layout, right: Layout) -> Layout {
        match self {
            Binary::Line => line(left, right),
            Binary::BlankLine => line(left, line(null(), right)),
            Binary::Comp(pad, brk) => comp(left, right, pad, brk),
        }
    }
}

#[derive(Debug)]
enum Token {
    Open,
    Close,
    Null,
    Unary(Unary),
    Text(String),
    Binary(Binary),
}

fn tokenize(src: &str) -> Result<Vec<(usize, Token)>, ParseError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let err = |message| ParseError {
            offset: start,
            message,
        };
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
                let mut data = String::new();
                i += 1;
                loop {
                    let Some(&c) = bytes.get(i) else {
                        return Err(err("unterminated string"));
                    };
                    i += 1;
                    match c {
                        b'"' => break,
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
                Token::Text(data)
            }
            b'a'..=b'z' => {
                while i < bytes.len() && bytes[i].is_ascii_lowercase() {
                    i += 1;
                }
                match &src[start..i] {
                    "null" => Token::Null,
                    "fix" => Token::Unary(Unary::Fix),
                    "grp" => Token::Unary(Unary::Grp),
                    "seq" => Token::Unary(Unary::Seq),
                    "nest" => Token::Unary(Unary::Nest),
                    "pack" => Token::Unary(Unary::Pack),
                    _ => return Err(err("unknown keyword")),
                }
            }
            _ => {
                // Longest match first, so `!&` and `@@` are not split.
                let two = bytes.get(i..i + 2);
                let (len, op) = match two {
                    Some(b"@@") => (2, Binary::BlankLine),
                    Some(b"!&") => (2, Binary::Comp(Pad::Unpadded, Break::Fixed)),
                    Some(b"!+") => (2, Binary::Comp(Pad::Padded, Break::Fixed)),
                    _ => match bytes[i] {
                        b'@' => (1, Binary::Line),
                        b'&' => (1, Binary::Comp(Pad::Unpadded, Break::Breakable)),
                        b'+' => (1, Binary::Comp(Pad::Padded, Break::Breakable)),
                        _ => return Err(err("unexpected character")),
                    },
                };
                i += len;
                Token::Binary(op)
            }
        };
        tokens.push((start, token));
    }
    Ok(tokens)
}

/// One parenthesized level: the operand chain built so far, the operators
/// between the operands, and a unary operator waiting for its operand.
struct Frame {
    operands: Vec<Layout>,
    ops: Vec<Binary>,
    unary: Option<Unary>,
}

impl Frame {
    fn new() -> Frame {
        Frame {
            operands: Vec::new(),
            ops: Vec::new(),
            unary: None,
        }
    }

    fn push_operand(&mut self, layout: Layout) {
        let layout = match self.unary.take() {
            Some(unary) => unary.apply(layout),
            None => layout,
        };
        self.operands.push(layout);
    }

    /// Folds the chain right-associatively: `a op1 b op2 c` is
    /// `a op1 (b op2 c)`.
    fn finish(self) -> Layout {
        let mut operands = self.operands.into_iter();
        let mut ops = self.ops.into_iter();
        let first = operands.next().expect("a finished frame has an operand");
        fold_right(first, &mut operands, &mut ops)
    }
}

fn fold_right(
    first: Layout,
    operands: &mut impl Iterator<Item = Layout>,
    ops: &mut impl Iterator<Item = Binary>,
) -> Layout {
    // Collect then fold from the end, so the chain stays a loop.
    let mut lefts = vec![first];
    let mut binops = Vec::new();
    for (op, operand) in ops.zip(operands) {
        binops.push(op);
        lefts.push(operand);
    }
    let mut result = lefts.pop().expect("at least the first operand");
    while let (Some(op), Some(left)) = (binops.pop(), lefts.pop()) {
        result = op.apply(left, result);
    }
    result
}

/// Parses a layout from its DSL form.
pub fn parse(src: &str) -> Result<Layout, ParseError> {
    let tokens = tokenize(src)?;
    let mut frames = vec![Frame::new()];
    // Whether the next token must be an operand (else an operator or `)`).
    let mut want_operand = true;
    let err = |offset, message| ParseError { offset, message };
    for (offset, token) in tokens {
        let frame = frames.last_mut().expect("the root frame is never popped");
        if want_operand {
            match token {
                Token::Null => frame.push_operand(null()),
                Token::Text(data) => frame.push_operand(text(data)),
                Token::Unary(unary) => {
                    if frame.unary.is_some() {
                        return Err(err(offset, "expected a primary expression"));
                    }
                    frame.unary = Some(unary);
                    continue;
                }
                Token::Open => {
                    frames.push(Frame::new());
                    continue;
                }
                Token::Close | Token::Binary(_) => {
                    return Err(err(offset, "expected a primary expression"));
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
                        return Err(err(offset, "unexpected )"));
                    }
                    let inner = frames.pop().expect("nested frame").finish();
                    frames.last_mut().expect("root frame").push_operand(inner);
                }
                _ => return Err(err(offset, "expected an operator")),
            }
        }
    }
    if want_operand {
        return Err(err(src.len(), "expected a primary expression"));
    }
    if frames.len() != 1 {
        return Err(err(src.len(), "expected )"));
    }
    Ok(frames.pop().expect("root frame").finish())
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
        assert_eq!(
            (e.offset(), e.message()),
            (6, "expected a primary expression")
        );
        let e = parse(r#"("a""#).unwrap_err();
        assert_eq!((e.offset(), e.message()), (4, "expected )"));
        let e = parse(r#""a")"#).unwrap_err();
        assert_eq!((e.offset(), e.message()), (3, "unexpected )"));
        let e = parse(r#""a" "b""#).unwrap_err();
        assert_eq!((e.offset(), e.message()), (4, "expected an operator"));
        let e = parse(r#"grp grp ("a")"#).unwrap_err();
        assert_eq!(
            (e.offset(), e.message()),
            (4, "expected a primary expression")
        );
        let e = parse(r#""abc"#).unwrap_err();
        assert_eq!((e.offset(), e.message()), (0, "unterminated string"));
        let e = parse("foo").unwrap_err();
        assert_eq!((e.offset(), e.message()), (0, "unknown keyword"));
        assert_eq!(
            parse("").unwrap_err().message(),
            "expected a primary expression"
        );
    }

    #[test]
    fn deep_parentheses_do_not_recurse() {
        let depth = 100_000;
        let src = format!("{}\"x\"{}", "(".repeat(depth), ")".repeat(depth));
        assert_eq!(fmt(&src, 2, 80), "x");
    }
}
