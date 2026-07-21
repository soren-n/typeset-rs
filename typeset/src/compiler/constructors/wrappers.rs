//! Constructors that wrap a layout in delimiters: `()`, `[]`, `{}`.

use super::basic::text;
use super::composition::comp;
use crate::compiler::types::Layout;

/// Wrap `layout` between the `open` and `close` delimiters using unpadded
/// compositions, so no spaces are introduced and the delimiters never break
/// apart from the content (the content may still break internally).
fn wrap(open: &str, close: &str, layout: Box<Layout>) -> Box<Layout> {
    comp(
        text(open),
        comp(layout, text(close), false, false),
        false,
        false,
    )
}

/// Wraps a layout in parentheses: `(content)`.
///
/// ```rust
/// use typeset::*;
/// let call = comp(text("f"), parens(join_with_commas(vec![text("a"), text("b")])), false, false);
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
