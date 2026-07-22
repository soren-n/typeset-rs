//! Layout constructors and convenience functions
//!
//! This module provides a clean API for building layout trees before passing them to
//! the compilation pipeline. These functions are the primary interface for creating
//! pretty-printed output with automatic line breaking and indentation.
//!
//! # Overview
//!
//! The constructor functions create different types of layout nodes:
//!
//! - **Basic constructors** - [`null`], [`text`]
//! - **Control constructors** - [`fix`], [`grp`], [`seq`], [`nest`], [`pack`]
//! - **Composition constructors** - [`line()`], [`comp`] and shortcuts [`pad`], [`unpad`], etc.
//! - **Convenience constructors** - [`space`], [`comma`], [`newline`], etc.
//! - **Joining functions** - [`join_with`], [`join_with_spaces`], etc.
//! - **Wrapping functions** - [`parens`], [`brackets`], [`braces`]
//!
//! # Example Usage
//!
//! ```rust
//! use typeset::*;
//!
//! // Build a function definition layout
//! let layout = comp(
//!     text("function"),
//!     nest(comp(
//!         text("name()"),
//!         braces(text("body")),
//!         Pad::Padded, Break::Breakable
//!     )),
//!     Pad::Padded, Break::Breakable
//! );
//!
//! let output = format_layout(layout, 2, 40);
//! ```

use crate::compiler::types::{Attr, Break, Layout, Pad};

// --- Basic constructors: the primitives ------------------------------------

/// The empty layout: produces no output and is neutral in compositions. Useful
/// as a placeholder when building layouts conditionally.
///
/// ```rust
/// use typeset::*;
/// let result = comp(null(), text("content"), Pad::Padded, Break::Breakable);
/// assert_eq!(format_layout(result, 2, 80), "content");
/// ```
pub fn null() -> Box<Layout> {
    Box::new(Layout::Null)
}

/// A text literal: the fundamental visible content. Text is a single unit that
/// never breaks across lines.
///
/// Accepts anything convertible into a `String` — a `&str` literal or an owned
/// `String` both work without an explicit conversion.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(text("Hello, world!"), 2, 80), "Hello, world!");
/// ```
pub fn text(data: impl Into<String>) -> Box<Layout> {
    Box::new(Layout::Text(data.into()))
}

// --- Control constructors: breaking, grouping, nesting, alignment ----------

/// Wraps a layout as a fixed unit that never breaks across lines.
///
/// ```rust
/// use typeset::*;
/// let expr = fix(comp(text("a"), comp(text(" + "), text("b"), Pad::Unpadded, Break::Breakable), Pad::Unpadded, Break::Breakable));
/// // Fixed content stays on one line even when narrower than its width.
/// assert_eq!(format_layout(expr, 2, 10), "a + b");
/// ```
pub fn fix(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Fix(layout))
}

/// Wraps a layout as a group: every breakable composition inside it breaks
/// together, all-or-nothing. Contrast [`seq`], where a break cascades forward.
///
/// ```rust
/// use typeset::*;
/// let args = grp(join_with_commas(vec![text("a"), text("b")]));
/// // When it fits, the group stays on one line.
/// assert_eq!(format_layout(args, 2, 80), "a, b");
/// ```
pub fn grp(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Grp(layout))
}

/// Wraps a layout as a sequence: once one composition breaks, every later one
/// in the sequence breaks too (a cascading break). Contrast [`grp`].
///
/// ```rust
/// use typeset::*;
/// let stmts = seq(join_with_lines(vec![text("a;"), text("b;")]));
/// assert_eq!(format_layout(stmts, 2, 80), "a;\nb;");
/// ```
pub fn seq(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Seq(layout))
}

/// Wraps a layout so that lines it breaks onto are indented by a fixed width
/// (the `tab` passed to rendering). Single-line content is unaffected.
///
/// ```rust
/// use typeset::*;
/// let call = comp(text("f("), comp(nest(text("x")), text(")"), Pad::Unpadded, Break::Breakable), Pad::Unpadded, Break::Breakable);
/// assert_eq!(format_layout(call, 2, 80), "f(x)");
/// ```
pub fn nest(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Nest(layout))
}

/// Wraps a layout so that lines it breaks onto align to the column where its
/// first element started (hanging indentation), rather than the fixed-width
/// indentation of [`nest`].
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(pack(text("x")), 2, 80), "x");
/// ```
pub fn pack(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Pack(layout))
}

// --- Composition constructors: combining two layouts -----------------------

/// A forced line break: `left` on one line, `right` on the next (respecting the
/// current indentation). Unlike [`comp`], this always breaks.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(line(text("First"), text("Second")), 2, 80), "First\nSecond");
/// ```
pub fn line(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Line(left, right))
}

/// The general composition of two layouts. [`Pad`] chooses whether a space
/// separates them when they share a line; [`Break`] chooses whether the
/// composition may break (`Break::Fixed` forbids it, like wrapping in
/// [`fix`](crate::fix)). When a breakable composition doesn't fit, the right
/// operand moves to the next line. The [`pad`]/[`unpad`]/[`fix_pad`]/[`fix_unpad`]
/// shortcuts name the four combinations.
///
/// ```rust
/// use typeset::*;
/// let padded = comp(text("function"), text("name()"), Pad::Padded, Break::Breakable);
/// assert_eq!(format_layout(padded, 2, 80), "function name()");
/// ```
pub fn comp(left: Box<Layout>, right: Box<Layout>, pad: Pad, brk: Break) -> Box<Layout> {
    Box::new(Layout::Comp(
        left,
        right,
        Attr {
            pad: pad.is_padded(),
            fix: brk.is_fixed(),
        },
    ))
}

/// Padded, breakable composition — `comp(left, right, Pad::Padded,
/// Break::Breakable)`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(pad(text("Hello"), text("world")), 2, 80), "Hello world");
/// ```
pub fn pad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, Pad::Padded, Break::Breakable)
}

/// Unpadded, breakable composition — `comp(left, right, Pad::Unpadded,
/// Break::Breakable)`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(unpad(text("prefix"), text("suffix")), 2, 80), "prefixsuffix");
/// ```
pub fn unpad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, Pad::Unpadded, Break::Breakable)
}

/// Padded composition that never breaks — `comp(left, right, Pad::Padded,
/// Break::Fixed)`.
///
/// ```rust
/// use typeset::*;
/// // Stays on one line even when narrower than its width.
/// assert_eq!(format_layout(fix_pad(text("!"), text("condition")), 2, 5), "! condition");
/// ```
pub fn fix_pad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, Pad::Padded, Break::Fixed)
}

/// Unpadded composition that never breaks — `comp(left, right, Pad::Unpadded,
/// Break::Fixed)`. Useful for compound tokens like `->` or `==`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(fix_unpad(text("-"), text(">")), 2, 80), "->");
/// ```
pub fn fix_unpad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, Pad::Unpadded, Break::Fixed)
}

// --- Convenience constructors: spaces, punctuation, line breaks ------------

/// A single space, equivalent to `text(" ")`.
///
/// ```rust
/// use typeset::*;
/// let spaced = comp(text("Hello"), comp(space(), text("world"), Pad::Unpadded, Break::Breakable), Pad::Unpadded, Break::Breakable);
/// assert_eq!(format_layout(spaced, 2, 80), "Hello world");
/// ```
pub fn space() -> Box<Layout> {
    text(" ")
}

/// A comma, `text(",")`. Usually reached via [`join_with_commas`].
///
/// ```rust
/// use typeset::*;
/// let items = join_with_commas(vec![text("a"), text("b"), text("c")]);
/// assert_eq!(format_layout(items, 2, 80), "a, b, c");
/// ```
pub fn comma() -> Box<Layout> {
    text(",")
}

/// A semicolon, `text(";")`.
///
/// ```rust
/// use typeset::*;
/// let statement = comp(text("let x = 5"), semicolon(), Pad::Unpadded, Break::Breakable);
/// assert_eq!(format_layout(statement, 2, 80), "let x = 5;");
/// ```
pub fn semicolon() -> Box<Layout> {
    text(";")
}

/// A line break with no content on either side, equivalent to `line(null(), null())`.
///
/// ```rust
/// use typeset::*;
/// let separated = comp(text("First line"), comp(newline(), text("Second line"), Pad::Unpadded, Break::Breakable), Pad::Unpadded, Break::Breakable);
/// assert_eq!(format_layout(separated, 2, 80), "First line\nSecond line");
/// ```
pub fn newline() -> Box<Layout> {
    line(null(), null())
}

/// A blank line (two consecutive breaks), equivalent to `line(line(null(), null()), null())`.
///
/// ```rust
/// use typeset::*;
/// let document = comp(text("Section 1"), comp(blank_line(), text("Section 2"), Pad::Unpadded, Break::Breakable), Pad::Unpadded, Break::Breakable);
/// assert_eq!(format_layout(document, 2, 80), "Section 1\n\nSection 2");
/// ```
pub fn blank_line() -> Box<Layout> {
    line(line(null(), null()), null())
}

// --- Joining: combine a collection with a separator ------------------------

/// Left-folds `layouts` with `combine`, returning [`null`] for an empty vector
/// and the sole element (untouched) for a singleton. Shared by [`join_with`]
/// and [`join_with_lines`], which differ only in how adjacent elements combine.
fn join_reduce(
    layouts: impl IntoIterator<Item = Box<Layout>>,
    combine: impl FnMut(Box<Layout>, Box<Layout>) -> Box<Layout>,
) -> Box<Layout> {
    layouts.into_iter().reduce(combine).unwrap_or_else(null)
}

/// Joins `layouts` with `separator` between each pair, via unpadded
/// compositions (the separator supplies its own spacing). Returns [`null`] for
/// an empty vector and the sole element for a singleton.
///
/// ```rust
/// use typeset::*;
/// let joined = join_with(vec![text("a"), text("b")], comp(comma(), space(), Pad::Unpadded, Break::Breakable));
/// assert_eq!(format_layout(joined, 2, 80), "a, b");
/// ```
pub fn join_with(layouts: Vec<Box<Layout>>, separator: Box<Layout>) -> Box<Layout> {
    join_reduce(layouts, move |acc, layout| {
        unpad(acc, unpad(separator.clone(), layout))
    })
}

/// Joins `layouts` with single spaces — `join_with(layouts, space())`.
///
/// ```rust
/// use typeset::*;
/// let sentence = join_with_spaces(vec![text("Hello"), text("world")]);
/// assert_eq!(format_layout(sentence, 2, 80), "Hello world");
/// ```
pub fn join_with_spaces(layouts: Vec<Box<Layout>>) -> Box<Layout> {
    join_with(layouts, space())
}

/// Joins `layouts` with `", "` separators — the standard comma-separated list.
///
/// ```rust
/// use typeset::*;
/// let params = join_with_commas(vec![text("x"), text("y"), text("z")]);
/// assert_eq!(format_layout(params, 2, 80), "x, y, z");
/// ```
pub fn join_with_commas(layouts: Vec<Box<Layout>>) -> Box<Layout> {
    join_with(layouts, unpad(comma(), space()))
}

/// Joins `layouts` with forced [`line()`] breaks — one element per line.
///
/// ```rust
/// use typeset::*;
/// let lines = join_with_lines(vec![text("a;"), text("b;")]);
/// assert_eq!(format_layout(lines, 2, 80), "a;\nb;");
/// ```
pub fn join_with_lines(layouts: Vec<Box<Layout>>) -> Box<Layout> {
    join_reduce(layouts, line)
}

// --- Wrappers: enclose a layout in delimiters ------------------------------

/// Wrap `layout` between the `open` and `close` delimiters using unpadded
/// compositions, so no spaces are introduced and the delimiters never break
/// apart from the content (the content may still break internally).
fn wrap(open: &str, close: &str, layout: Box<Layout>) -> Box<Layout> {
    unpad(text(open), unpad(layout, text(close)))
}

/// Wraps a layout in parentheses: `(content)`.
///
/// ```rust
/// use typeset::*;
/// let call = comp(text("f"), parens(join_with_commas(vec![text("a"), text("b")])), Pad::Unpadded, Break::Breakable);
/// assert_eq!(format_layout(call, 2, 80), "f(a, b)");
/// ```
pub fn parens(layout: Box<Layout>) -> Box<Layout> {
    wrap("(", ")", layout)
}

/// Wraps a layout in square brackets: `[content]`.
///
/// ```rust
/// use typeset::*;
/// let array = brackets(join_with_commas(vec![text("1"), text("2"), text("3")]));
/// assert_eq!(format_layout(array, 2, 80), "[1, 2, 3]");
/// ```
pub fn brackets(layout: Box<Layout>) -> Box<Layout> {
    wrap("[", "]", layout)
}

/// Wraps a layout in curly braces: `{content}`. Commonly combined with
/// [`nest`](crate::nest) for indented block content.
///
/// ```rust
/// use typeset::*;
/// let block = braces(text("body"));
/// assert_eq!(format_layout(block, 2, 80), "{body}");
/// ```
pub fn braces(layout: Box<Layout>) -> Box<Layout> {
    wrap("{", "}", layout)
}

// --- High-level one-step formatting ----------------------------------------

/// Compiles and renders a layout in one step: `render(compile(layout), tab,
/// width)`.
///
/// `tab` is the number of spaces per indentation level; `width` is the target
/// line width for breaking decisions (not a hard limit — fixed content may
/// exceed it). To format the same layout repeatedly, prefer [`crate::compile`]
/// once with [`crate::render()`] per call. The pipeline is iterative, so it does
/// not overflow the stack on deep input.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(text("Hello, world!"), 2, 80), "Hello, world!");
/// ```
pub fn format_layout(layout: Box<Layout>, tab: usize, width: usize) -> String {
    use crate::compiler::pipeline;
    let doc = pipeline::compile(layout);
    pipeline::render(doc, tab, width)
}
