//! Constructors for combining two layouts: the forced line break, the general
//! composition, and the four padding/fixing shortcuts.

use crate::compiler::types::{Attr, Layout};

/// A forced line break: `left` on one line, `right` on the next (respecting the
/// current indentation). Unlike [`comp`], this always breaks.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(line(text("First"), text("Second")), 2, 80), "First\nSecond");
/// ```
pub fn line(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    Box::new(Layout::Line(left, right))
}

/// The general composition of two layouts. `pad` inserts a space between them
/// when they share a line; `fix` forbids breaking (like wrapping in
/// [`fix`](crate::fix)). When a breakable composition doesn't fit, the right
/// operand moves to the next line. The [`pad`]/[`unpad`]/[`fix_pad`]/[`fix_unpad`]
/// shortcuts cover the four boolean combinations.
///
/// ```rust
/// use typeset::*;
/// let padded = comp(text("function"), text("name()"), true, false);
/// assert_eq!(format_layout(padded, 2, 80), "function name()");
/// ```
pub fn comp(left: Box<Layout>, right: Box<Layout>, pad: bool, fix: bool) -> Box<Layout> {
    Box::new(Layout::Comp(left, right, Attr { pad, fix }))
}

/// Padded, breakable composition — `comp(left, right, true, false)`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(pad(text("Hello"), text("world")), 2, 80), "Hello world");
/// ```
pub fn pad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, true, false)
}

/// Unpadded, breakable composition — `comp(left, right, false, false)`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(unpad(text("prefix"), text("suffix")), 2, 80), "prefixsuffix");
/// ```
pub fn unpad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, false, false)
}

/// Padded composition that never breaks — `comp(left, right, true, true)`.
///
/// ```rust
/// use typeset::*;
/// // Stays on one line even when narrower than its width.
/// assert_eq!(format_layout(fix_pad(text("!"), text("condition")), 2, 5), "! condition");
/// ```
pub fn fix_pad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, true, true)
}

/// Unpadded composition that never breaks — `comp(left, right, false, true)`.
/// Useful for compound tokens like `->` or `==`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(fix_unpad(text("-"), text(">")), 2, 80), "->");
/// ```
pub fn fix_unpad(left: Box<Layout>, right: Box<Layout>) -> Box<Layout> {
    comp(left, right, false, true)
}
