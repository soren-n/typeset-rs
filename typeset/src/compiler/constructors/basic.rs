//! The primitive layout constructors: the empty layout and text.

use crate::compiler::types::Layout;

/// The empty layout: produces no output and is neutral in compositions. Useful
/// as a placeholder when building layouts conditionally.
///
/// ```rust
/// use typeset::*;
/// let result = comp(null(), text("content".to_string()), true, false);
/// assert_eq!(format_layout(result, 2, 80), "content");
/// ```
pub fn null() -> Box<Layout> {
    Box::new(Layout::Null)
}

/// A text literal: the fundamental visible content. Text is a single unit that
/// never breaks across lines.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(text("Hello, world!".to_string()), 2, 80), "Hello, world!");
/// ```
pub fn text(data: String) -> Box<Layout> {
    Box::new(Layout::Text(data))
}

/// [`text`] taking a `&str`, equivalent to `text(s.to_string())`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(text_str("Hello"), 2, 80), "Hello");
/// ```
pub fn text_str(s: &str) -> Box<Layout> {
    text(s.to_string())
}
