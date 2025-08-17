//! Layout wrapper constructors
//!
//! This module provides constructors for wrapping layouts with common
//! delimiters: parentheses, square brackets, and curly braces. These
//! utilities create properly formatted delimited structures.

use super::basic::text_str;
use super::composition::comp;
use crate::compiler::types::Layout;

/// Wraps a layout in parentheses.
///
/// This convenience function surrounds the given layout with parentheses,
/// using unpadded compositions to ensure no extra spaces are added between
/// the parentheses and the content. This is commonly used for function calls,
/// mathematical expressions, grouping operations, and similar constructs.
///
/// # Parameters
///
/// * `layout` - The layout to wrap in parentheses
///
/// # Returns
///
/// A layout wrapped in parentheses: `(content)`
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Function call
/// let call = comp(
///     text_str("function"),
///     parens(join_with_commas(vec![
///         text_str("arg1"),
///         text_str("arg2")
///     ])),
///     false, false
/// );
///
/// assert_eq!(format_layout(call, 2, 80), "function(arg1, arg2)");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Mathematical expression grouping
/// let expr = comp(
///     text_str("a +"),
///     parens(comp(
///         text_str("b * c"),
///         text_str(""),
///         true, false
///     )),
///     true, false
/// );
///
/// // Output: "a + (b * c)"
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Nested parentheses
/// let nested = parens(
///     parens(text_str("inner"))
/// );
///
/// assert_eq!(format_layout(nested, 2, 80), "((inner))");
/// ```
///
/// # Behavior
///
/// - Uses unpadded compositions (`pad=false`) to avoid spaces around parentheses
/// - The wrapped content can break internally based on its own structure
/// - Parentheses themselves never break apart from their content
/// - No additional spacing is introduced
///
/// # See Also
///
/// - [`brackets`] - For wrapping in square brackets `[]`
/// - [`braces`] - For wrapping in curly braces `{}`
/// - [`comp`] - The underlying composition constructor used
pub fn parens(layout: Box<Layout>) -> Box<Layout> {
    comp(
        text_str("("),
        comp(layout, text_str(")"), false, false),
        false,
        false,
    )
}

/// Wraps a layout in square brackets.
///
/// This convenience function surrounds the given layout with square brackets,
/// using unpadded compositions to ensure no extra spaces are added. This is
/// commonly used for array literals, indexing operations, attribute syntax,
/// and other bracket-delimited constructs.
///
/// # Parameters
///
/// * `layout` - The layout to wrap in square brackets
///
/// # Returns
///
/// A layout wrapped in square brackets: `[content]`
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Array literal
/// let array = brackets(
///     join_with_commas(vec![
///         text_str("1"),
///         text_str("2"),
///         text_str("3")
///     ])
/// );
///
/// assert_eq!(format_layout(array, 2, 80), "[1, 2, 3]");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Array indexing
/// let indexing = comp(
///     text_str("array"),
///     brackets(text_str("index")),
///     false, false
/// );
///
/// assert_eq!(format_layout(indexing, 2, 80), "array[index]");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Multi-line array with grouping
/// let long_array = brackets(
///     grp(nest(join_with_commas(vec![
///         text_str("very_long_element_name_1"),
///         text_str("very_long_element_name_2"),
///         text_str("very_long_element_name_3")
///     ])))
/// );
///
/// // When it fits: "[very_long_element_name_1, very_long_element_name_2, very_long_element_name_3]"
/// // When it breaks:
/// // [
/// //   very_long_element_name_1,
/// //   very_long_element_name_2,
/// //   very_long_element_name_3
/// // ]
/// ```
///
/// # Behavior
///
/// - Uses unpadded compositions (`pad=false`) to avoid spaces around brackets
/// - The wrapped content can break internally based on its own structure
/// - Brackets themselves never break apart from their content
/// - No additional spacing is introduced
///
/// # See Also
///
/// - [`parens`] - For wrapping in parentheses `()`
/// - [`braces`] - For wrapping in curly braces `{}`
/// - [`comp`] - The underlying composition constructor used
pub fn brackets(layout: Box<Layout>) -> Box<Layout> {
    comp(
        text_str("["),
        comp(layout, text_str("]"), false, false),
        false,
        false,
    )
}

/// Wraps a layout in curly braces.
///
/// This convenience function surrounds the given layout with curly braces,
/// using unpadded compositions to ensure no extra spaces are added. This is
/// commonly used for code blocks, object literals, set notation, scope
/// delimiters, and other brace-delimited constructs.
///
/// # Parameters
///
/// * `layout` - The layout to wrap in curly braces
///
/// # Returns
///
/// A layout wrapped in curly braces: `{content}`
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Simple code block
/// let block = braces(
///     nest(join_with_lines(vec![
///         text_str("let x = 1;"),
///         text_str("println!(\"{}\", x);")
///     ]))
/// );
///
/// let result = format_layout(block, 2, 40);
/// // Output:
/// // {
/// //   let x = 1;
/// //   println!("{}", x);
/// // }
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // JSON-like object
/// let object = braces(
///     nest(join_with_commas(vec![
///         comp(text_str("\"name\""), text_str(": \"John\""), true, false),
///         comp(text_str("\"age\""), text_str(": 30"), true, false)
///     ]))
/// );
///
/// // When it fits: '{"name": "John", "age": 30}'
/// // When it breaks:
/// // {
/// //   "name": "John",
/// //   "age": 30
/// // }
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Function definition
/// let function = comp(
///     text_str("fn main()"),
///     braces(
///         nest(text_str("println!(\"Hello, world!\");"))
///     ),
///     true, false
/// );
///
/// // Output:
/// // fn main() {
/// //   println!("Hello, world!");
/// // }
/// ```
///
/// # Behavior
///
/// - Uses unpadded compositions (`pad=false`) to avoid spaces around braces
/// - The wrapped content can break internally based on its own structure
/// - Braces themselves never break apart from their content
/// - Commonly combined with [`nest`] for proper indentation of block content
/// - No additional spacing is introduced
///
/// # See Also
///
/// - [`parens`] - For wrapping in parentheses `()`
/// - [`brackets`] - For wrapping in square brackets `[]`
/// - [`nest`] - Often used inside braces for proper indentation
/// - [`join_with_lines`] - Common for multi-statement blocks
pub fn braces(layout: Box<Layout>) -> Box<Layout> {
    comp(
        text_str("{"),
        comp(layout, text_str("}"), false, false),
        false,
        false,
    )
}
