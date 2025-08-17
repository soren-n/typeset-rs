//! Basic layout constructors
//!
//! This module provides the fundamental building blocks for creating layouts:
//! empty layouts, text content, and basic text utilities. These are the
//! most primitive constructors that form the foundation for all other layouts.

use crate::compiler::types::Layout;

/// Creates an empty layout node.
///
/// The null layout produces no output and serves as a neutral element in compositions.
/// It's useful as a placeholder or when building layouts conditionally.
///
/// # Returns
///
/// A boxed [`Layout::Null`] that produces no visible output.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Use null as a placeholder in conditional layouts
/// let should_include_prefix = false;
/// let layout = if should_include_prefix {
///     text("prefix: ".to_string())
/// } else {
///     null()
/// };
///
/// // Null is neutral in compositions
/// let result = comp(null(), text("content".to_string()), true, false);
/// assert_eq!(format_layout(result, 2, 80), "content");
/// ```
///
/// # See Also
///
/// - [`text`] - For creating text content
/// - [`comp`] - For composing layouts together
pub fn null() -> Box<Layout> {
    Box::new(Layout::Null)
}

/// Creates a text layout containing the given string.
///
/// Text layouts are the fundamental building blocks that produce visible output.
/// The text content is treated as a single unit that cannot be broken across lines.
///
/// # Parameters
///
/// * `data` - The string content to include in the layout
///
/// # Returns
///
/// A boxed [`Layout::Text`] containing the provided string.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Create simple text
/// let hello = text("Hello, world!".to_string());
/// assert_eq!(format_layout(hello, 2, 80), "Hello, world!");
///
/// // Text preserves whitespace and special characters
/// let code = text("  fn main() {".to_string());
/// assert_eq!(format_layout(code, 2, 80), "  fn main() {");
/// ```
///
/// # See Also
///
/// - [`text_str`] - Convenience function that takes `&str`
/// - [`comp`] - For combining text with other layouts
/// - [`fix`] - For ensuring text never breaks
pub fn text(data: String) -> Box<Layout> {
    Box::new(Layout::Text(data))
}

/// Creates a text layout from a string slice.
///
/// This is a convenience function that converts a `&str` to a `String` and creates
/// a text layout. It's equivalent to `text(s.to_string())` but more ergonomic.
///
/// # Parameters
///
/// * `s` - The string slice to convert into a text layout
///
/// # Returns
///
/// A boxed [`Layout::Text`] containing the string.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // More convenient than text() for string literals
/// let greeting = text_str("Hello");
/// assert_eq!(format_layout(greeting, 2, 80), "Hello");
///
/// // Equivalent to using text() with to_string()
/// let a = text_str("content");
/// let b = text("content".to_string());
/// assert_eq!(
///     format_layout(a, 2, 80),
///     format_layout(b, 2, 80)
/// );
/// ```
///
/// # See Also
///
/// - [`text`] - The underlying function that takes `String`
/// - Common text constructors: [`space`], [`comma`], [`semicolon`]
pub fn text_str(s: &str) -> Box<Layout> {
    text(s.to_string())
}
