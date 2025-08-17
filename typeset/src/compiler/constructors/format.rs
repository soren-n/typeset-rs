//! High-level formatting utilities
//!
//! This module provides high-level functions that combine layout construction,
//! compilation, and rendering into convenient single-step operations.

use crate::compiler::types::Layout;

/// Compiles and renders a layout to a formatted string in one step.
///
/// This convenience function combines the compilation and rendering phases into
/// a single operation. It takes a layout tree, compiles it through the
/// multi-pass optimization pipeline, and renders it to a string with the
/// specified formatting parameters. This is the most common way to use the
/// typeset library for simple formatting tasks.
///
/// # Parameters
///
/// * `layout` - The layout tree to format
/// * `tab` - The number of spaces to use for each indentation level
/// * `width` - The target line width for line-breaking decisions
///
/// # Returns
///
/// A formatted string with appropriate line breaks and indentation.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Simple formatting
/// let simple = text_str("Hello, world!");
/// let result = format_layout(simple, 2, 80);
/// assert_eq!(result, "Hello, world!");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Complex structure with breaking
/// let func = comp(
///     text_str("function"),
///     parens(
///         grp(join_with_commas(vec![
///             text_str("param1: Type1"),
///             text_str("param2: Type2"),
///             text_str("param3: Type3")
///         ]))
///     ),
///     true, false
/// );
///
/// // Wide output (width=80):
/// let wide = format_layout(func.clone(), 2, 80);
/// // "function(param1: Type1, param2: Type2, param3: Type3)"
///
/// // Narrow output (width=30):
/// let narrow = format_layout(func, 2, 30);
/// // "function(param1: Type1,
/// //           param2: Type2,
/// //           param3: Type3)"
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Nested structure with indentation
/// let block = braces(
///     nest(join_with_lines(vec![
///         text_str("statement1();"),
///         text_str("statement2();"),
///         nest(braces(
///             nest(text_str("nested_statement();"))
///         ))
///     ]))
/// );
///
/// let result = format_layout(block, 4, 40);
/// // Output:
/// // {
/// //     statement1();
/// //     statement2();
/// //     {
/// //         nested_statement();
/// //     }
/// // }
/// ```
///
/// # Parameters
///
/// ## `tab` Parameter
/// - Controls indentation width for [`nest`] constructors
/// - Common values: 2 (compact), 4 (standard), 8 (traditional)
/// - Affects readability vs. horizontal space trade-off
///
/// ## `width` Parameter
/// - Target line width for breaking decisions
/// - Not a hard limit - fixed content may exceed this
/// - Guides the layout algorithm's breaking choices
/// - Common values: 80 (traditional), 100 (modern), 120 (wide)
///
/// # Performance
///
/// - This function allocates and performs the full compilation pipeline
/// - For repeated formatting with the same parameters, consider using
///   [`crate::compile`] once and [`crate::render()`] multiple times
/// - Uses the fast, unsafe compilation path - may panic on stack overflow
/// - For safety-critical code, use [`crate::compile_safe`] and [`crate::render()`] separately
///
/// # See Also
///
/// - [`crate::compile`] + [`crate::render()`] - For separate compilation and rendering
/// - [`crate::compile_safe`] - For safe compilation with error handling
/// - The main library documentation for pipeline details
pub fn format_layout(layout: Box<Layout>, tab: usize, width: usize) -> String {
    // Use the new modular pipeline
    use crate::compiler::pipeline;
    let doc = pipeline::compile(layout);
    pipeline::render(doc, tab, width)
}
