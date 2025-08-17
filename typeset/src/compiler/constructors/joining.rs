//! Layout joining constructors
//!
//! This module provides constructors for joining collections of layouts
//! with various separators: spaces, commas, lines, or custom separators.
//! These utilities make it easy to build lists, sequences, and repeated structures.

use super::basic::null;
use super::composition::{comp, line};
use super::text_utils::{comma, space};
use crate::compiler::types::Layout;

/// Joins a collection of layouts with a custom separator.
///
/// This is the generic joining function that combines multiple layouts with a
/// specified separator between each element. The separator is padded (has spaces
/// around it when on the same line) in the resulting composition. This function
/// forms the basis for other joining utilities.
///
/// # Parameters
///
/// * `layouts` - Vector of layouts to join together
/// * `separator` - Layout to place between each pair of elements
///
/// # Returns
///
/// A single layout with elements separated by the separator, or [`null`] if empty.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Join with custom separator
/// let items = join_with(vec![
///     text_str("apple"),
///     text_str("banana"),
///     text_str("cherry")
/// ], text_str(" | "));
///
/// assert_eq!(format_layout(items, 2, 80), "apple | banana | cherry");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Join with arrow separators for pipelines
/// let pipeline = join_with(vec![
///     text_str("input"),
///     text_str("process1"),
///     text_str("process2"),
///     text_str("output")
/// ], text_str(" -> "));
///
/// // Output: "input -> process1 -> process2 -> output"
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Complex separators with breaking behavior
/// let statements = join_with(vec![
///     text_str("statement1;"),
///     text_str("statement2;"),
///     text_str("statement3;")
/// ], newline());
///
/// // Each statement on its own line
/// ```
///
/// # Behavior
///
/// - Empty vector returns [`null`]
/// - Single element vector returns that element unchanged
/// - Multiple elements are joined with padded compositions
/// - The separator is placed between consecutive elements (N-1 separators for N elements)
/// - Both the main composition and separator composition use `pad=true, fix=false`
///
/// # Edge Cases
///
/// ```rust
/// use typeset::*;
///
/// // Empty vector
/// let empty = join_with(vec![], comma());
/// assert_eq!(format_layout(empty, 2, 80), "");
///
/// // Single element
/// let single = join_with(vec![text_str("alone")], comma());
/// assert_eq!(format_layout(single, 2, 80), "alone");
/// ```
///
/// # See Also
///
/// - [`join_with_spaces`] - Joins with space separators
/// - [`join_with_commas`] - Joins with comma separators
/// - [`join_with_lines`] - Joins with line breaks
/// - [`comp`] - The underlying composition function used
pub fn join_with(mut layouts: Vec<Box<Layout>>, separator: Box<Layout>) -> Box<Layout> {
    match layouts.len() {
        0 => null(),
        1 => layouts.pop().unwrap(),
        _ => {
            let mut result = layouts.remove(0);
            for layout in layouts {
                result = comp(
                    result,
                    comp(separator.clone(), layout, false, false),
                    false,
                    false,
                );
            }
            result
        }
    }
}

/// Joins a collection of layouts with space separators.
///
/// This convenience function takes a vector of layouts and combines them with
/// single spaces between each element. It's equivalent to
/// `join_with(layouts, space())` and is commonly used for word-like elements
/// or horizontal layout composition.
///
/// # Parameters
///
/// * `layouts` - Vector of layouts to join with spaces
///
/// # Returns
///
/// A single layout with all input layouts separated by spaces, or [`null`] if empty.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Join words with spaces
/// let sentence = join_with_spaces(vec![
///     text_str("Hello"),
///     text_str("beautiful"),
///     text_str("world")
/// ]);
///
/// assert_eq!(format_layout(sentence, 2, 80), "Hello beautiful world");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Join different types of layouts
/// let mixed = join_with_spaces(vec![
///     text_str("fn"),
///     text_str("main()"),
///     braces(text_str("println!(\"Hello\");"))
/// ]);
///
/// // When it fits: "fn main() {println!(\"Hello\");}"
/// // When it breaks, the braces content may break internally
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Empty vector returns null
/// let empty = join_with_spaces(vec![]);
/// assert_eq!(format_layout(empty, 2, 80), "");
///
/// // Single element returns itself
/// let single = join_with_spaces(vec![text_str("alone")]);
/// assert_eq!(format_layout(single, 2, 80), "alone");
/// ```
///
/// # See Also
///
/// - [`join_with_commas`] - For comma-separated lists
/// - [`join_with`] - Generic joining with any separator
/// - [`space`] - The separator used by this function
pub fn join_with_spaces(layouts: Vec<Box<Layout>>) -> Box<Layout> {
    join_with(layouts, space())
}

/// Joins a collection of layouts with comma separators.
///
/// This convenience function takes a vector of layouts and combines them with
/// comma-space separators (", ") between each element. It's equivalent to
/// `join_with(layouts, comma())` but note that the underlying implementation
/// adds both comma and space. This is the standard way to create comma-separated
/// lists like function parameters, array elements, or object properties.
///
/// # Parameters
///
/// * `layouts` - Vector of layouts to join with commas
///
/// # Returns
///
/// A single layout with all input layouts separated by ", ", or [`null`] if empty.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Function parameter list
/// let params = join_with_commas(vec![
///     text_str("x: i32"),
///     text_str("y: i32"),
///     text_str("z: String")
/// ]);
///
/// let function = comp(
///     text_str("fn example("),
///     comp(params, text_str(")"), false, false),
///     false, false
/// );
///
/// // Output: "fn example(x: i32, y: i32, z: String)"
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Array elements that may break
/// let array = brackets(
///     grp(join_with_commas(vec![
///         text_str("element1"),
///         text_str("element2"),
///         text_str("element3")
///     ]))
/// );
///
/// // Fits: "[element1, element2, element3]"
/// // Breaks: "[element1,
/// //          element2,
/// //          element3]"
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // JSON-like object properties
/// let object = braces(
///     nest(join_with_commas(vec![
///         comp(text_str("\"name\""), text_str(": \"John\""), true, false),
///         comp(text_str("\"age\""), text_str(": 30"), true, false)
///     ]))
/// );
/// ```
///
/// # See Also
///
/// - [`join_with_spaces`] - For space-separated elements
/// - [`join_with`] - Generic joining with any separator  
/// - [`comma`] - The separator character used
/// - [`grp`] - Often used to control breaking behavior of comma-separated lists
pub fn join_with_commas(layouts: Vec<Box<Layout>>) -> Box<Layout> {
    join_with(layouts, comp(comma(), space(), false, false))
}

/// Joins a collection of layouts with line breaks.
///
/// This function combines multiple layouts by placing each on its own line using
/// [`line()`] constructors. Unlike [`join_with`] which uses compositions that may
/// or may not break, this function creates unconditional line breaks between
/// all elements. It's ideal for statement sequences, block content, or any
/// vertically-aligned content.
///
/// # Parameters
///
/// * `layouts` - Vector of layouts to join with line breaks
///
/// # Returns
///
/// A single layout with each element on its own line, or [`null`] if empty.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Statement sequence
/// let statements = join_with_lines(vec![
///     text_str("let x = 1;"),
///     text_str("let y = 2;"),
///     text_str("println!(\"{}, {}\", x, y);")
/// ]);
///
/// let result = format_layout(statements, 2, 80);
/// // Output:
/// // let x = 1;
/// // let y = 2;
/// // println!("{}, {}", x, y);
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Block content with nesting
/// let block = braces(
///     nest(join_with_lines(vec![
///         text_str("first_action();"),
///         text_str("second_action();"),
///         text_str("return result;")
///     ]))
/// );
///
/// // Output:
/// // {
/// //   first_action();
/// //   second_action();
/// //   return result;
/// // }
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Combine with other constructors
/// let function = line(
///     text_str("fn example() {"),
///     line(
///         join_with_lines(vec![
///             nest(text_str("statement1;")),
///             nest(text_str("statement2;"))
///         ]),
///         text_str("}")
///     )
/// );
/// ```
///
/// # Behavior
///
/// - Empty vector returns [`null`]
/// - Single element vector returns that element unchanged  
/// - Multiple elements are connected with [`line()`] constructors
/// - Each element appears on its own line with proper indentation
/// - Line breaks are unconditional (not affected by width constraints)
///
/// # Edge Cases
///
/// ```rust
/// use typeset::*;
///
/// // Empty vector
/// let empty = join_with_lines(vec![]);
/// assert_eq!(format_layout(empty, 2, 80), "");
///
/// // Single element  
/// let single = join_with_lines(vec![text_str("alone")]);
/// assert_eq!(format_layout(single, 2, 80), "alone");
/// ```
///
/// # See Also
///
/// - [`join_with`] - Generic joining that may break based on width
/// - [`line()`] - The underlying line break constructor
/// - [`newline`], [`blank_line`] - For manual line break insertion
/// - [`seq`] - Often used with line-based layouts for breaking control
pub fn join_with_lines(mut layouts: Vec<Box<Layout>>) -> Box<Layout> {
    match layouts.len() {
        0 => null(),
        1 => layouts.pop().unwrap(),
        _ => {
            let mut result = layouts.remove(0);
            for layout in layouts {
                result = line(result, layout);
            }
            result
        }
    }
}
