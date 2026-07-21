//! Convenience constructors for common text elements: spaces, punctuation, and
//! line breaks.

use super::basic::{null, text};
use super::composition::line;
use crate::compiler::types::Layout;

/// A single space, equivalent to `text(" ")`.
///
/// ```rust
/// use typeset::*;
/// let spaced = comp(text("Hello"), comp(space(), text("world"), false, false), false, false);
/// assert_eq!(format_layout(spaced, 2, 80), "Hello world");
/// ```
pub fn space() -> Box<Layout> {
    text(" ")
}

/// A comma, `text(",")`. Usually reached via [`join_with_commas`](crate::join_with_commas).
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
/// let statement = comp(text("let x = 5"), semicolon(), false, false);
/// assert_eq!(format_layout(statement, 2, 80), "let x = 5;");
/// ```
pub fn semicolon() -> Box<Layout> {
    text(";")
}

/// A line break with no content on either side, equivalent to `line(null(), null())`.
///
/// ```rust
/// use typeset::*;
/// let separated = comp(text("First line"), comp(newline(), text("Second line"), false, false), false, false);
/// assert_eq!(format_layout(separated, 2, 80), "First line\nSecond line");
/// ```
pub fn newline() -> Box<Layout> {
    line(null(), null())
}

/// A blank line (two consecutive breaks), equivalent to `line(line(null(), null()), null())`.
///
/// ```rust
/// use typeset::*;
/// let document = comp(text("Section 1"), comp(blank_line(), text("Section 2"), false, false), false, false);
/// assert_eq!(format_layout(document, 2, 80), "Section 1\n\nSection 2");
/// ```
pub fn blank_line() -> Box<Layout> {
    line(line(null(), null()), null())
}
