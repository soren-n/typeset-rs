//! Constructors that wrap a layout in delimiters: `()`, `[]`, `{}`.

use super::basic::text_str;
use super::composition::comp;
use crate::compiler::types::Layout;

/// Wrap `layout` between the `open` and `close` delimiters using unpadded
/// compositions, so no spaces are introduced and the delimiters never break
/// apart from the content (the content may still break internally).
fn _wrap(open: &str, close: &str, layout: Box<Layout>) -> Box<Layout> {
    comp(
        text_str(open),
        comp(layout, text_str(close), false, false),
        false,
        false,
    )
}

/// Wraps a layout in parentheses: `(content)`.
///
/// ```rust
/// use typeset::*;
/// let call = comp(text_str("f"), parens(join_with_commas(vec![text_str("a"), text_str("b")])), false, false);
/// assert_eq!(format_layout(call, 2, 80), "f(a, b)");
/// ```
pub fn parens(layout: Box<Layout>) -> Box<Layout> {
    _wrap("(", ")", layout)
}

/// Wraps a layout in square brackets: `[content]`.
///
/// ```rust
/// use typeset::*;
/// let array = brackets(join_with_commas(vec![text_str("1"), text_str("2"), text_str("3")]));
/// assert_eq!(format_layout(array, 2, 80), "[1, 2, 3]");
/// ```
pub fn brackets(layout: Box<Layout>) -> Box<Layout> {
    _wrap("[", "]", layout)
}

/// Wraps a layout in curly braces: `{content}`. Commonly combined with
/// [`nest`](crate::nest) for indented block content.
///
/// ```rust
/// use typeset::*;
/// let block = braces(text_str("body"));
/// assert_eq!(format_layout(block, 2, 80), "{body}");
/// ```
pub fn braces(layout: Box<Layout>) -> Box<Layout> {
    _wrap("{", "}", layout)
}
