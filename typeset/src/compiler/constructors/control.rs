//! Constructors that control breaking, grouping, nesting, and alignment.

use crate::compiler::types::Layout;

/// Wraps a layout as a fixed unit that never breaks across lines.
///
/// ```rust
/// use typeset::*;
/// let expr = fix(comp(text_str("a"), comp(text_str(" + "), text_str("b"), false, false), false, false));
/// // Fixed content stays on one line even when narrower than its width.
/// assert_eq!(format_layout(expr, 2, 10), "a + b");
/// ```
pub fn fix(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Fix(layout))
}

/// Wraps a layout as a group: every breakable composition inside it breaks
/// together, all-or-nothing. Contrast [`seq`], where a break cascades forward.
///
/// ```rust
/// use typeset::*;
/// let args = grp(join_with_commas(vec![text_str("a"), text_str("b")]));
/// // When it fits, the group stays on one line.
/// assert_eq!(format_layout(args, 2, 80), "a, b");
/// ```
pub fn grp(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Grp(layout))
}

/// Wraps a layout as a sequence: once one composition breaks, every later one
/// in the sequence breaks too (a cascading break). Contrast [`grp`].
///
/// ```rust
/// use typeset::*;
/// let stmts = seq(join_with_lines(vec![text_str("a;"), text_str("b;")]));
/// assert_eq!(format_layout(stmts, 2, 80), "a;\nb;");
/// ```
pub fn seq(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Seq(layout))
}

/// Wraps a layout so that lines it breaks onto are indented by a fixed width
/// (the `tab` passed to rendering). Single-line content is unaffected.
///
/// ```rust
/// use typeset::*;
/// let call = comp(text_str("f("), comp(nest(text_str("x")), text_str(")"), false, false), false, false);
/// assert_eq!(format_layout(call, 2, 80), "f(x)");
/// ```
pub fn nest(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Nest(layout))
}

/// Wraps a layout so that lines it breaks onto align to the column where its
/// first element started (hanging indentation), rather than the fixed-width
/// indentation of [`nest`].
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(pack(text_str("x")), 2, 80), "x");
/// ```
pub fn pack(layout: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Pack(layout))
}
