//! Layout control constructors
//!
//! This module provides constructors that control the structural behavior
//! of layouts: how they break, group, nest, and align. These constructors
//! modify the breaking and indentation behavior of their content.

use crate::compiler::types::Layout;

/// Wraps a layout to prevent it from breaking across lines.
///
/// The `fix` constructor treats its content as a fixed unit that must appear
/// on a single line. This is useful for preserving the structure of code
/// elements that should never be broken, such as operators, keywords, or
/// small expressions.
///
/// # Parameters
///
/// * `layout` - The layout to treat as fixed
///
/// # Returns
///
/// A boxed [`Layout::Fix`] that prevents line breaking within the wrapped layout.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Ensure an operator stays on one line
/// let operator = fix(text_str(" -> "));
///
/// // Fix can prevent breaking of compound expressions
/// let expr = fix(comp(
///     text_str("a"),
///     comp(text_str(" + "), text_str("b"), false, false),
///     false, false
/// ));
///
/// // The expression will always render as "a + b" on one line
/// assert_eq!(format_layout(expr, 2, 10), "a + b");
/// ```
///
/// # Behavior
///
/// - Fixed layouts never break, regardless of width constraints
/// - Use sparingly - over-fixing can create layouts that exceed line width
/// - Compositions marked with `fix=true` behave similarly
///
/// # See Also
///
/// - [`grp`] - For layouts that break together as a group
/// - [`comp`] - The `fix` parameter controls breaking behavior
/// - [`fix_pad`], [`fix_unpad`] - Composition shortcuts with fixing
pub fn fix(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Fix(layout))
}

/// Wraps a layout to control group breaking behavior.
///
/// The `grp` constructor creates a group where all breakable elements within
/// the group either break together or stay together. This is essential for
/// maintaining consistent formatting in structures like argument lists,
/// array elements, or object properties.
///
/// # Parameters
///
/// * `layout` - The layout to treat as a breaking group
///
/// # Returns
///
/// A boxed [`Layout::Grp`] that controls group breaking behavior.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Group function arguments - they break together or not at all
/// let args = grp(join_with_commas(vec![
///     text_str("arg1"),
///     text_str("arg2"),
///     text_str("arg3")
/// ]));
///
/// let func_call = comp(
///     text_str("function"),
///     parens(args),
///     false, false
/// );
///
/// // With enough width: "function(arg1, arg2, arg3)"
/// // When narrow, all arguments break:
/// // function(
/// //   arg1,
/// //   arg2,
/// //   arg3
/// // )
/// ```
///
/// # Behavior
///
/// - All compositions within the group make breaking decisions together
/// - Prevents partial breaking that could result in inconsistent formatting
/// - Essential for maintaining visual consistency in structured data
///
/// # See Also
///
/// - [`seq`] - For sequential breaking where one break forces all breaks
/// - [`fix`] - For preventing any breaking
/// - [`nest`] - For adding indentation to broken groups
pub fn grp(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Grp(layout))
}

/// Wraps a layout for sequential breaking behavior.
///
/// The `seq` constructor creates a sequence where if any composition within
/// the sequence breaks, then all subsequent compositions in the sequence also
/// break. This creates a "cascading" break effect useful for statement lists,
/// block structures, or any content where breaking should propagate forward.
///
/// # Parameters
///
/// * `layout` - The layout to treat as a breaking sequence
///
/// # Returns
///
/// A boxed [`Layout::Seq`] that implements sequential breaking.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Sequential breaking for statement-like structures
/// let statements = seq(join_with_lines(vec![
///     comp(text_str("let x = "), text_str("value1;"), true, false),
///     comp(text_str("let y = "), text_str("value2;"), true, false),
///     comp(text_str("let z = "), text_str("value3;"), true, false)
/// ]));
///
/// // If one statement needs to break due to width constraints,
/// // all following statements will also break, creating:
/// // let x =
/// //   value1;
/// // let y =
/// //   value2;
/// // let z =
/// //   value3;
/// ```
///
/// # Behavior
///
/// - Breaking propagates forward through the sequence
/// - Earlier elements can stay unbroken even if later ones break
/// - Useful for maintaining consistent indentation in code blocks
/// - Different from [`grp`] which breaks all-or-nothing
///
/// # See Also
///
/// - [`grp`] - For all-or-nothing group breaking
/// - [`line()`] - For explicit line breaks
/// - [`join_with_lines`] - Often used with seq for statement sequences
pub fn seq(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Seq(layout))
}

/// Wraps a layout to add fixed-width indentation.
///
/// The `nest` constructor adds indentation to its wrapped content when that
/// content spans multiple lines. The indentation width is determined by the
/// `tab` parameter passed to the rendering function. This is the standard
/// way to create nested, hierarchical layouts.
///
/// # Parameters
///
/// * `layout` - The layout to indent when it breaks across lines
///
/// # Returns
///
/// A boxed [`Layout::Nest`] that adds indentation to broken content.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Create a nested block structure
/// let block = braces(
///     nest(join_with_lines(vec![
///         text_str("statement1;"),
///         text_str("statement2;"),
///         text_str("statement3;")
///     ]))
/// );
///
/// let result = format_layout(block, 2, 15); // 2-space indentation, narrow width
/// // Output:
/// // {
/// //   statement1;
/// //   statement2;
/// //   statement3;
/// // }
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Nest function parameters
/// let func = comp(
///     text_str("function("),
///     comp(
///         nest(join_with_commas(vec![
///             text_str("param1: Type1"),
///             text_str("param2: Type2")
///         ])),
///         text_str(")"),
///         false, false
///     ),
///     false, false
/// );
/// ```
///
/// # Behavior
///
/// - Indentation only applies when content breaks across multiple lines
/// - The indentation amount is controlled by the `tab` parameter in rendering
/// - Nesting levels accumulate - nested `nest` calls create deeper indentation
/// - Single-line content is not affected
///
/// # See Also
///
/// - [`pack`] - For alignment-based indentation instead of fixed-width
/// - [`format_layout`] - The `tab` parameter controls indentation width
/// - [`braces`], [`parens`] - Often combined with nest for block structures
pub fn nest(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Nest(layout))
}

/// Wraps a layout to add alignment-based indentation.
///
/// The `pack` constructor aligns its content to the column position where the
/// first text element appears, rather than using fixed-width indentation like
/// [`nest`]. This creates "hanging" indentation that aligns with the start
/// of the content, commonly used for parameter lists and similar structures.
///
/// # Parameters
///
/// * `layout` - The layout to align based on first element position
///
/// # Returns
///
/// A boxed [`Layout::Pack`] that provides alignment-based indentation.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Pack aligns to the first element position
/// let func_call = comp(
///     text_str("very_long_function_name("),
///     pack(join_with_commas(vec![
///         text_str("first_parameter"),
///         text_str("second_parameter"),
///         text_str("third_parameter")
///     ])),
///     false, false
/// );
///
/// // When broken, parameters align with the first parameter:
/// // very_long_function_name(first_parameter,
/// //                         second_parameter,
/// //                         third_parameter)
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Compare with nest (fixed indentation):
/// let with_nest = comp(
///     text_str("func("),
///     nest(text_str("param")),
///     false, false
/// );
/// // Produces: func(
/// //             param   (fixed indentation)
///
/// let with_pack = comp(
///     text_str("func("),
///     pack(text_str("param")),
///     false, false  
/// );
/// // Produces: func(param   (aligns to position after 'func(')
/// ```
///
/// # Behavior
///
/// - Indentation aligns to the column where content starts
/// - More flexible than fixed-width [`nest`] for varying prefixes
/// - Particularly useful for function calls, method chaining, and similar patterns
/// - Alignment is based on the first text element encountered
///
/// # See Also
///
/// - [`nest`] - For fixed-width indentation
/// - [`join_with_commas`] - Often used within pack for parameter lists
/// - [`comp`] - Used to combine prefixes with packed content
pub fn pack(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Pack(layout))
}
