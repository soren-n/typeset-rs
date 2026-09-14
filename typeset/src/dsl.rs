//! The layout DSL.
//!
//! One grammar, two front ends: [`Layout`]'s `FromStr` reads it from a
//! string at run time, and the `typeset-parser` crate's `layout!` macro
//! reads it from Rust tokens at compile time by feeding the same token
//! parser (the hidden `grammar` module, a contract between the two crates).
//! `Layout`'s `Display` prints it.
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
//! use typeset::Layout;
//!
//! let layout: Layout = r#"grp ("a" + "b") @ nest ("c" & "d")"#.parse()?;
//! assert_eq!(layout.to_string(), r#"grp ("a" + "b") @ nest ("c" & "d")"#);
//! assert_eq!(layout.compile().render(2, 80), "a b\n  cd");
//! # Ok::<(), typeset::dsl::ParseError>(())
//! ```
//!
//! The parser is iterative: parenthesis depth costs heap, never native stack.

#[doc(hidden)]
pub mod grammar;

use self::grammar::{Binary, Build, Token, Unary};
use crate::constructors::{comp, fix, grp, line, nest, null, pack, seq, text};
use crate::layout::{Break, Layout, Pad};
use std::fmt;

/// Why a DSL text failed to parse (`message`), and where: the position of
/// the offending token, or the end of the input when it ends inside an
/// expression. At run time the position is the byte offset into the
/// source; the `layout!` macro reports a span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError<P = usize> {
    pub at: P,
    pub message: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.at)
    }
}

impl std::error::Error for ParseError {}

// --- Printing ---------------------------------------------------------------

/// A node of a tree as the DSL printer sees it: a text, a unary operator
/// over a node, or two nodes under a binary operator.
pub(crate) enum Shape<'a, N> {
    Text(&'a str),
    Unary(Unary, N),
    Binary(N, Binary, N),
}

/// Prints the tree under `root` in the DSL, reading each node's shape with
/// `shape`. Every binary operator has one precedence level and associates
/// right, and a unary operator takes a primary, so a left operand or a
/// wrapped node is parenthesized when it is not a text. Iterative: depth
/// costs heap, never native stack.
pub(crate) fn write_dsl<'a, N: Copy>(
    f: &mut fmt::Formatter,
    root: N,
    shape: impl Fn(N) -> Shape<'a, N>,
) -> fmt::Result {
    /// A pending piece of output: a node (parenthesized or not) or a
    /// literal fragment.
    enum Piece<N> {
        Node(N, bool),
        Str(&'static str),
    }
    let is_text = |node: N| matches!(shape(node), Shape::Text(_));
    // Pieces are pushed in reverse so they pop in reading order.
    let mut stack = vec![Piece::Node(root, false)];
    while let Some(piece) = stack.pop() {
        let (node, paren) = match piece {
            Piece::Str(s) => {
                f.write_str(s)?;
                continue;
            }
            Piece::Node(node, paren) => (node, paren),
        };
        if paren {
            stack.push(Piece::Str(")"));
        }
        match shape(node) {
            Shape::Text(text) => write_text(f, text)?,
            Shape::Unary(op, child) => {
                stack.push(Piece::Node(child, !is_text(child)));
                stack.push(Piece::Str(" "));
                stack.push(Piece::Str(op.keyword()));
            }
            Shape::Binary(left, op, right) => {
                let left_binary = matches!(shape(left), Shape::Binary(..));
                stack.push(Piece::Node(right, false));
                stack.push(Piece::Str(" "));
                stack.push(Piece::Str(op.symbol()));
                stack.push(Piece::Str(" "));
                stack.push(Piece::Node(left, left_binary));
            }
        }
        if paren {
            stack.push(Piece::Str("("));
        }
    }
    Ok(())
}

/// A DSL string literal: the DSL's escapes and every other character raw.
fn write_text(f: &mut fmt::Formatter, text: &str) -> fmt::Result {
    f.write_str("\"")?;
    for c in text.chars() {
        match c {
            '\\' => f.write_str("\\\\")?,
            '"' => f.write_str("\\\"")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            '\0' => f.write_str("\\0")?,
            c => write!(f, "{c}")?,
        }
    }
    f.write_str("\"")
}

// --- The run-time front end ------------------------------------------------

/// The run-time front end has no variables.
#[derive(Debug, Clone, Copy)]
enum NoVar {}

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
    fn line(&mut self, left: Layout, right: Layout) -> Layout {
        line(left, right)
    }
    fn comp(&mut self, left: Layout, right: Layout, pad: Pad, brk: Break) -> Layout {
        comp(left, right, pad, brk)
    }
}

/// Parses a layout from its DSL form.
impl std::str::FromStr for Layout {
    type Err = ParseError;
    fn from_str(src: &str) -> Result<Layout, ParseError> {
        grammar::parse_tokens(tokenize(src)?, src.len(), &mut Constructors)
    }
}

fn tokenize(src: &str) -> Result<Vec<(usize, Token<NoVar>)>, ParseError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let err = |message| ParseError { at: start, message };
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

/// Scans the string literal starting at `start` in `src`, returning its
/// text and the offset just past its closing quote.
fn scan_text(src: &str, start: usize) -> Result<(String, usize), ParseError> {
    let bytes = src.as_bytes();
    let err = |message| ParseError { at: start, message };
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

    fn parse(src: &str) -> Result<Layout, ParseError> {
        src.parse()
    }

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
        use grammar::parse_text;
        assert_eq!(parse_text(r#""a\"b""#).unwrap(), "a\"b");
        let e = parse_text(r#"r"a""#).unwrap_err();
        assert_eq!((e.at, e.message), (0, "expected a string literal"));
        let e = parse_text(r#""a"b"#).unwrap_err();
        assert_eq!((e.at, e.message), (3, "expected the end of the literal"));
        let e = parse_text(r#""\u{e9}""#).unwrap_err();
        assert_eq!((e.at, e.message), (0, "unknown escape"));
    }
}
