//! Layout composition constructors
//!
//! This module provides constructors for combining layouts with various
//! breaking and spacing behaviors. These are the core building blocks
//! for creating complex layouts from simpler parts.

use crate::compiler::types::{Attr, Layout};

/// Creates a forced line break between two layouts.
///
/// The `line` constructor places its left layout on one line and its right
/// layout on the next line, with appropriate indentation. This creates an
/// unconditional line break, unlike compositions which may or may not break
/// depending on available width.
///
/// # Parameters
///
/// * `left` - Layout to appear before the line break
/// * `right` - Layout to appear after the line break (on the next line)
///
/// # Returns
///
/// A boxed [`Layout::Line`] that forces a line break between the layouts.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Simple line break
/// let two_lines = line(
///     text_str("First line"),
///     text_str("Second line")
/// );
///
/// assert_eq!(format_layout(two_lines, 2, 80), "First line\nSecond line");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Build multi-line structures
/// let code_block = line(
///     text_str("if condition {"),
///     line(
///         nest(text_str("do_something();")),
///         text_str("}")
///     )
/// );
///
/// // Output:
/// // if condition {
/// //   do_something();
/// // }
/// ```
///
/// # Behavior
///
/// - Always creates a line break, regardless of available width
/// - Respects current indentation level for the right layout
/// - Can be chained to create multiple line breaks
/// - The right layout inherits the current nesting context
///
/// # See Also
///
/// - [`newline`] - Convenience function for line breaks with empty content
/// - [`blank_line`] - Creates double line breaks
/// - [`join_with_lines`] - Joins multiple layouts with line breaks
/// - [`comp`] - For conditional breaking based on width
pub fn line(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Line(left, right))
}

/// Creates a composition that may break across lines based on width constraints.
///
/// The `comp` constructor is the fundamental building block for creating layouts
/// that adapt to available width. It combines two layouts with configurable
/// spacing and breaking behavior. This is the most flexible constructor and the
/// basis for most other layout combinations.
///
/// # Parameters
///
/// * `left` - The left layout in the composition
/// * `right` - The right layout in the composition  
/// * `pad` - Whether to add a space between layouts when they fit on one line
/// * `fix` - Whether to prevent this composition from breaking across lines
///
/// # Returns
///
/// A boxed [`Layout::Comp`] with the specified breaking and padding behavior.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Padded composition (adds space when on one line)
/// let padded = comp(
///     text_str("function"),
///     text_str("name()"),
///     true,  // pad = true
///     false  // fix = false (can break)
/// );
///
/// // Fits on one line: "function name()"
/// // When broken: "function\nname()"
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Unpadded composition (no space)
/// let unpadded = comp(
///     text_str("["),
///     comp(
///         text_str("content"),
///         text_str("]"),
///         false, false
///     ),
///     false, // pad = false
///     false  // fix = false
/// );
///
/// // Always renders as: "[content]"
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Fixed composition (never breaks)
/// let fixed = comp(
///     text_str("operator"),
///     text_str("operand"),
///     true,  // pad = true
///     true   // fix = true (never breaks)
/// );
///
/// // Always on one line: "operator operand"
/// ```
///
/// # Parameters
///
/// ## `pad` Parameter
/// - `true`: Adds a single space between layouts when they fit on one line
/// - `false`: No space added - layouts are directly adjacent
///
/// ## `fix` Parameter  
/// - `true`: This composition never breaks, similar to wrapping in [`fix`]
/// - `false`: This composition can break based on width constraints
///
/// # Behavior
///
/// - When both layouts fit on the current line, they're placed adjacent (with optional padding)
/// - When they don't fit, a line break is inserted and the right layout moves to the next line
/// - Breaking decisions consider the entire layout context, not just this composition
/// - Fixed compositions (`fix=true`) never break regardless of width
///
/// # See Also
///
/// - [`pad`], [`unpad`] - Convenience functions for common padding patterns
/// - [`fix_pad`], [`fix_unpad`] - Convenience functions with fixing
/// - [`line()`] - For unconditional line breaks
/// - [`fix`] - Alternative way to prevent breaking
pub fn comp(left: Box<Layout>, right: Box<Layout>, pad: bool, fix: bool) -> Box<Layout> {
    Box::new(Layout::Comp(left, right, Attr { pad, fix }))
}

/// Creates a padded composition between two layouts.
///
/// This convenience function creates a composition with padding enabled and
/// fixing disabled. When the layouts fit on the same line, a space is inserted
/// between them. When they don't fit, a line break is inserted instead.
/// This is equivalent to `comp(left, right, true, false)`.
///
/// # Parameters
///
/// * `left` - The left layout in the composition
/// * `right` - The right layout in the composition
///
/// # Returns
///
/// A padded, breakable composition of the two layouts.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Simple padded composition
/// let spaced = pad(
///     text_str("Hello"),
///     text_str("world")
/// );
///
/// assert_eq!(format_layout(spaced, 2, 80), "Hello world");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Breaking behavior when narrow
/// let breakable = pad(
///     text_str("very_long_identifier"),
///     text_str("another_long_identifier")
/// );
///
/// // Wide: "very_long_identifier another_long_identifier"
/// // Narrow: "very_long_identifier\nanother_long_identifier"
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Chain multiple padded compositions
/// let chain = pad(
///     text_str("first"),
///     pad(
///         text_str("second"),
///         text_str("third")
///     )
/// );
///
/// // Output: "first second third" (when it fits)
/// ```
///
/// # Behavior
///
/// - **Padding**: `true` - Adds a space when layouts are on the same line
/// - **Fixing**: `false` - Allows breaking when width constraints require it
/// - Equivalent to the most common composition pattern
/// - Breaking decisions are made based on available width
///
/// # See Also
///
/// - [`unpad`] - Composition without spaces
/// - [`fix_pad`] - Padded composition that never breaks
/// - [`comp`] - The underlying composition function
/// - [`join_with_spaces`] - For joining multiple layouts with padding
pub fn pad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, true, false)
}

/// Creates an unpadded composition between two layouts.
///
/// This convenience function creates a composition with padding disabled and
/// fixing disabled. The layouts are placed directly adjacent when on the same
/// line, with no space between them. When they don't fit, a line break is
/// inserted. This is equivalent to `comp(left, right, false, false)`.
///
/// # Parameters
///
/// * `left` - The left layout in the composition
/// * `right` - The right layout in the composition
///
/// # Returns
///
/// An unpadded, breakable composition of the two layouts.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Direct adjacency without spaces
/// let adjacent = unpad(
///     text_str("prefix"),
///     text_str("suffix")
/// );
///
/// assert_eq!(format_layout(adjacent, 2, 80), "prefixsuffix");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Building bracket-like constructs
/// let bracketed = unpad(
///     text_str("["),
///     unpad(
///         text_str("content"),
///         text_str("]")
///     )
/// );
///
/// assert_eq!(format_layout(bracketed, 2, 80), "[content]");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Method chaining or dot notation
/// let chained = unpad(
///     text_str("object"),
///     unpad(
///         text_str("."),
///         text_str("method()")
///     )
/// );
///
/// assert_eq!(format_layout(chained, 2, 80), "object.method()");
/// ```
///
/// # Behavior
///
/// - **Padding**: `false` - No space added when layouts are on the same line
/// - **Fixing**: `false` - Allows breaking when width constraints require it
/// - Useful for creating compound tokens or syntactic constructs
/// - Breaking still occurs when width constraints are exceeded
///
/// # See Also
///
/// - [`pad`] - Composition with spaces
/// - [`fix_unpad`] - Unpadded composition that never breaks
/// - [`comp`] - The underlying composition function
/// - Wrapper functions like [`parens`], [`brackets`], [`braces`] use unpadded compositions
pub fn unpad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, false, false)
}

/// Creates a fixed, padded composition between two layouts.
///
/// This convenience function creates a composition with both padding and fixing
/// enabled. The layouts are always placed on the same line with a space between
/// them, regardless of width constraints. This is equivalent to
/// `comp(left, right, true, true)` and is useful for constructs that must
/// never be broken apart.
///
/// # Parameters
///
/// * `left` - The left layout in the composition
/// * `right` - The right layout in the composition
///
/// # Returns
///
/// A fixed, padded composition that never breaks.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Keyword-identifier pairs that should stay together
/// let declaration = fix_pad(
///     text_str("let"),
///     text_str("variable")
/// );
///
/// // Always renders as: "let variable"
/// // Even in very narrow widths, this won't break
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Operators that should stay with their operands
/// let operation = fix_pad(
///     text_str("!"),
///     text_str("condition")
/// );
///
/// assert_eq!(format_layout(operation, 2, 5), "! condition");
/// // Note: This will exceed the width limit of 5 but won't break
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Type annotations
/// let typed = comp(
///     text_str("value:"),
///     fix_pad(text_str("&"), text_str("str")),
///     true, false
/// );
///
/// // The "& str" part always stays together
/// ```
///
/// # Behavior
///
/// - **Padding**: `true` - Always adds a space between layouts
/// - **Fixing**: `true` - Never breaks, regardless of width constraints
/// - Use with caution - can cause lines to exceed width limits
/// - Overrides all breaking decisions for this composition
/// - The space is always present, never replaced by a line break
///
/// # Use Cases
///
/// - Keyword-identifier pairs ("let x", "fn name")
/// - Type annotations ("&str", "`Vec<T>`")
/// - Operators with operands ("!flag", "*ptr")
/// - Short phrases that lose meaning when broken
///
/// # See Also
///
/// - [`pad`] - Padded composition that can break
/// - [`fix_unpad`] - Fixed composition without spaces
/// - [`fix`] - Alternative way to prevent breaking
/// - [`comp`] - The underlying composition function
pub fn fix_pad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, true, true)
}

/// Creates a fixed, unpadded composition between two layouts.
///
/// This convenience function creates a composition with padding disabled but
/// fixing enabled. The layouts are always placed directly adjacent on the same
/// line with no space between them, regardless of width constraints. This is
/// equivalent to `comp(left, right, false, true)` and is useful for compound
/// tokens or syntactic elements that must never be separated.
///
/// # Parameters
///
/// * `left` - The left layout in the composition
/// * `right` - The right layout in the composition
///
/// # Returns
///
/// A fixed, unpadded composition that never breaks or adds spaces.
///
/// # Examples
///
/// ```rust
/// use typeset::*;
///
/// // Compound operators that must stay together
/// let arrow = fix_unpad(
///     text_str("-"),
///     text_str(">")
/// );
///
/// assert_eq!(format_layout(arrow, 2, 80), "->");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // String prefixes or suffixes
/// let raw_string = fix_unpad(
///     text_str("r"),
///     text_str("\"content\"")
/// );
///
/// assert_eq!(format_layout(raw_string, 2, 80), "r\"content\"");
/// ```
///
/// ```rust
/// use typeset::*;
///
/// // Multi-character operators
/// let equality = fix_unpad(
///     text_str("="),
///     text_str("=")
/// );
///
/// // Use in larger expressions
/// let comparison = pad(
///     text_str("a"),
///     pad(equality, text_str("b"))
/// );
///
/// assert_eq!(format_layout(comparison, 2, 80), "a == b");
/// ```
///
/// # Behavior
///
/// - **Padding**: `false` - No space added between layouts
/// - **Fixing**: `true` - Never breaks, regardless of width constraints
/// - Creates indivisible compound tokens
/// - Use with caution - can cause lines to exceed width limits
/// - The layouts are always directly adjacent
///
/// # Use Cases
///
/// - Multi-character operators ("==", "!=", "->", "<=")
/// - Prefixed literals ("0x", "0b", raw strings)
/// - Compound symbols that lose meaning when separated
/// - Token sequences that form single logical units
///
/// # See Also
///
/// - [`unpad`] - Unpadded composition that can break
/// - [`fix_pad`] - Fixed composition with spaces
/// - [`fix`] - Alternative way to prevent breaking
/// - [`comp`] - The underlying composition function
pub fn fix_unpad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, false, true)
}
