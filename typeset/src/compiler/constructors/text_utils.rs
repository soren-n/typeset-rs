//! Text utility constructors
//!
//! This module provides convenience constructors for common text elements
//! and symbols: spaces, punctuation, and line breaks. These utilities
//! make it easier to build layouts with standard formatting elements.

use super::basic::{null, text_str};
use super::composition::line;

/// Creates a single space character layout.
///
/// This is a convenience function equivalent to `text_str(" ")`. It's commonly
/// used in compositions and as a separator in joining operations.
///
/// # Returns
///
/// A boxed [`Layout::Text`] containing a single space.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Use space in compositions
/// let spaced = comp(
///     text_str("Hello"),
///     comp(space(), text_str("world"), false, false),
///     false, false
/// );
///
/// assert_eq!(format_layout(spaced, 2, 80), "Hello world");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Equivalent to padded composition
/// let explicit = comp(text_str("A"), comp(space(), text_str("B"), false, false), false, false);
/// let padded = comp(text_str("A"), text_str("B"), true, false);
///
/// assert_eq!(
///     format_layout(explicit, 2, 80),
///     format_layout(padded, 2, 80)
/// );
/// ```
///
/// # See Also
///
/// - [`join_with_spaces`] - Joins layouts with spaces
/// - [`comp`] - The `pad=true` parameter provides automatic spacing
pub fn space() -> Box<crate::compiler::types::Layout> {
    text_str(" ")
}

/// Creates a comma character layout.
///
/// This is a convenience function for the common case of comma separators in
/// lists, parameter lists, object properties, and other comma-separated structures.
///
/// # Returns
///
/// A boxed [`Layout::Text`] containing a comma.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Build a list with explicit commas
/// let list = comp(
///     text_str("a"),
///     comp(
///         comma(),
///         comp(space(), text_str("b"), false, false),
///         false, false
///     ),
///     false, false
/// );
///
/// assert_eq!(format_layout(list, 2, 80), "a, b");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // More commonly used with joining functions
/// let items = join_with_commas(vec![
///     text_str("first"),
///     text_str("second"),
///     text_str("third")
/// ]);
///
/// assert_eq!(format_layout(items, 2, 80), "first, second, third");
/// ```
///
/// # See Also
///
/// - [`join_with_commas`] - Joins layouts with comma separators
/// - [`semicolon`] - For semicolon separators
/// - [`space`] - Often used after commas
pub fn comma() -> Box<crate::compiler::types::Layout> {
    text_str(",")
}

/// Creates a semicolon character layout.
///
/// This is a convenience function for semicolon separators, commonly used in
/// statement terminators, CSS properties, and other semicolon-separated structures.
///
/// # Returns
///
/// A boxed [`Layout::Text`] containing a semicolon.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Statement with semicolon terminator
/// let statement = comp(
///     text_str("let x = 5"),
///     semicolon(),
///     false, false
/// );
///
/// assert_eq!(format_layout(statement, 2, 80), "let x = 5;");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Multiple statements
/// let statements = join_with_lines(vec![
///     comp(text_str("first_statement()"), semicolon(), false, false),
///     comp(text_str("second_statement()"), semicolon(), false, false)
/// ]);
///
/// // Output:
/// // first_statement();
/// // second_statement();
/// ```
///
/// # See Also
///
/// - [`comma`] - For comma separators
/// - [`join_with_lines`] - Often used for statement sequences
pub fn semicolon() -> Box<crate::compiler::types::Layout> {
    text_str(";")
}

/// Creates a simple line break.
///
/// This is a convenience function that creates a line break without any content
/// on either side. It's equivalent to `line(null(), null())` and provides a
/// clean way to insert line breaks in layouts.
///
/// # Returns
///
/// A boxed [`Layout::Line`] with empty content on both sides.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Simple line break between content
/// let separated = comp(
///     text_str("First line"),
///     comp(newline(), text_str("Second line"), false, false),
///     false, false
/// );
///
/// assert_eq!(format_layout(separated, 2, 80), "First line\nSecond line");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Multiple line breaks
/// let spaced = comp(
///     text_str("Content"),
///     comp(
///         newline(),
///         comp(newline(), text_str("More content"), false, false),
///         false, false
///     ),
///     false, false
/// );
///
/// // Creates a blank line between content
/// ```
///
/// # See Also
///
/// - [`blank_line`] - For double line breaks
/// - [`line()`] - For line breaks with content on both sides
/// - [`join_with_lines`] - For joining multiple layouts with line breaks
pub fn newline() -> Box<crate::compiler::types::Layout> {
    line(null(), null())
}

/// Creates a blank line (double line break).
///
/// This is a convenience function that creates two consecutive line breaks,
/// resulting in a blank line in the output. It's equivalent to
/// `line(line(null(), null()), null())` and is useful for separating sections
/// of content with whitespace.
///
/// # Returns
///
/// A boxed [`Layout::Line`] that produces a blank line.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Separate sections with blank lines
/// let document = comp(
///     text_str("Section 1"),
///     comp(
///         blank_line(),
///         text_str("Section 2"),
///         false, false
///     ),
///     false, false
/// );
///
/// let result = format_layout(document, 2, 80);
/// // Output:
/// // Section 1
/// //
/// // Section 2
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Multiple blank lines for spacing
/// let spaced = join_with(vec![
///     text_str("Header"),
///     text_str("Body"),
///     text_str("Footer")
/// ], blank_line());
///
/// // Creates well-separated sections
/// ```
///
/// # See Also
///
/// - [`newline`] - For single line breaks
/// - [`line()`] - The underlying line break constructor
/// - [`join_with_lines`] - For single-line separation
pub fn blank_line() -> Box<crate::compiler::types::Layout> {
    line(line(null(), null()), null())
}
