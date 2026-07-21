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
