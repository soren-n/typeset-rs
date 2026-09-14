//! Layout constructors.
//!
//! These functions are the way to build a [`Layout`]: primitives ([`null`],
//! [`text`]), wrappers ([`fix`], [`grp`], [`seq`], [`nest`], [`pack`]),
//! compositions ([`comp`], [`line()`] and the [`pad`]/[`unpad`]/[`fix_pad`]/
//! [`fix_unpad`] shortcuts), and the joins over collections.
//!
//! ```rust
//! use typeset::*;
//!
//! let args = pack(seq(join_with_commas([text("x"), text("y")])));
//! let doc = unpad(text("f("), fix_unpad(args, text(")"))).compile();
//! assert_eq!(doc.render(2, 80), "f(x, y)");
//! assert_eq!(doc.render(2, 5), "f(x,\n  y)");
//! ```

use crate::layout::{Break, Layout, LayoutNode, Pad};

// --- Primitives ------------------------------------------------------------

/// The empty layout: produces no output and is neutral in compositions. Useful
/// as a placeholder when building layouts conditionally. It is the empty
/// text, and vanishes from the document along with any wrappers on it.
///
/// ```rust
/// use typeset::*;
/// let result = pad(null(), text("content"));
/// assert_eq!(result.compile().render(2, 80), "content");
/// ```
pub fn null() -> Layout {
    text("")
}

/// A text literal: the fundamental visible content. Text is a single unit that
/// never breaks across lines.
///
/// Accepts anything convertible into a `String` — a `&str` literal or an owned
/// `String` both work without an explicit conversion.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(text("Hello, world!").compile().render(2, 80), "Hello, world!");
/// ```
pub fn text(data: impl Into<String>) -> Layout {
    Layout::text(data.into())
}

// --- Wrappers: breaking, grouping, nesting, alignment ----------------------

/// Wraps a layout as a fixed unit that never breaks across lines.
///
/// ```rust
/// use typeset::*;
/// let expr = fix(unpad(text("a"), unpad(text(" + "), text("b"))));
/// // Fixed content stays on one line even when narrower than its width.
/// assert_eq!(expr.compile().render(2, 3), "a + b");
/// ```
pub fn fix(layout: Layout) -> Layout {
    layout.unary(LayoutNode::Fix)
}

/// Wraps a layout as a group: every breakable composition inside it breaks
/// together, all-or-nothing. Contrast [`seq`], where a break cascades forward.
///
/// ```rust
/// use typeset::*;
/// let args = grp(join_with_commas([text("a"), text("b")]));
/// // When it fits, the group stays on one line.
/// assert_eq!(args.compile().render(2, 80), "a, b");
/// ```
pub fn grp(layout: Layout) -> Layout {
    layout.unary(LayoutNode::Grp)
}

/// Wraps a layout as a sequence: once one composition breaks, every later one
/// in the sequence breaks too (a cascading break). Contrast [`grp`].
///
/// ```rust
/// use typeset::*;
/// let words = seq(join_with_spaces([text("one"), text("two"), text("three")]));
/// assert_eq!(words.compile().render(2, 9), "one\ntwo\nthree");
/// ```
pub fn seq(layout: Layout) -> Layout {
    if layout.has_line {
        layout.unary(LayoutNode::Broken)
    } else {
        layout.unary(LayoutNode::Seq)
    }
}

/// Wraps a layout so that lines it breaks onto are indented by a fixed width
/// (the `tab` passed to rendering). Single-line content is unaffected.
///
/// ```rust
/// use typeset::*;
/// let doc = pad(text("f"), nest(join_with_spaces([text("x"), text("y")]))).compile();
/// assert_eq!(doc.render(2, 80), "f x y");
/// assert_eq!(doc.render(2, 4), "f x\n  y");
/// ```
pub fn nest(layout: Layout) -> Layout {
    layout.unary(LayoutNode::Nest)
}

/// Wraps a layout so that lines it breaks onto align to the column where its
/// first element started (hanging indentation), rather than the fixed-width
/// indentation of [`nest`].
///
/// ```rust
/// use typeset::*;
/// let call = pad(text("f"), pack(join_with_spaces([text("x"), text("y")])));
/// assert_eq!(call.compile().render(2, 4), "f x\n  y");
/// ```
pub fn pack(layout: Layout) -> Layout {
    layout.unary(LayoutNode::Pack)
}

// --- Compositions: combining two layouts -----------------------------------

/// A forced line break: `left` on one line, `right` on the next (respecting the
/// current indentation). Unlike [`comp`], this always breaks.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(line(text("First"), text("Second")).compile().render(2, 80), "First\nSecond");
/// // A blank line is a line break onto the empty layout.
/// assert_eq!(line(text("a"), line(null(), text("b"))).compile().render(2, 80), "a\n\nb");
/// ```
pub fn line(left: Layout, right: Layout) -> Layout {
    let mut layout = Layout::binary(left, right, LayoutNode::Line);
    layout.has_line = true;
    layout
}

/// The general composition of two layouts. [`Pad`] chooses whether a space
/// separates them when they share a line; [`Break`] chooses whether the
/// composition may break (`Break::Fixed` forbids it, like wrapping in
/// [`fix`]). When a breakable composition doesn't fit, the right
/// operand moves to the next line. The [`pad`]/[`unpad`]/[`fix_pad`]/[`fix_unpad`]
/// shortcuts name the four combinations.
///
/// ```rust
/// use typeset::*;
/// let padded = comp(text("function"), text("name()"), Pad::Padded, Break::Breakable);
/// assert_eq!(padded.compile().render(2, 80), "function name()");
/// ```
pub fn comp(left: Layout, right: Layout, pad: Pad, brk: Break) -> Layout {
    Layout::binary(left, right, |l, r| LayoutNode::Comp(l, r, pad, brk))
}

/// Padded, breakable composition — `comp(left, right, Pad::Padded,
/// Break::Breakable)`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(pad(text("Hello"), text("world")).compile().render(2, 80), "Hello world");
/// ```
pub fn pad(left: Layout, right: Layout) -> Layout {
    comp(left, right, Pad::Padded, Break::Breakable)
}

/// Unpadded, breakable composition — `comp(left, right, Pad::Unpadded,
/// Break::Breakable)`.
///
/// ```rust
/// use typeset::*;
/// assert_eq!(unpad(text("prefix"), text("suffix")).compile().render(2, 80), "prefixsuffix");
/// ```
pub fn unpad(left: Layout, right: Layout) -> Layout {
    comp(left, right, Pad::Unpadded, Break::Breakable)
}

/// Padded composition that never breaks — `comp(left, right, Pad::Padded,
/// Break::Fixed)`. The fix binds the rightmost literal of `left` to the
/// leftmost literal of `right`; anything else in either operand may still
/// break.
///
/// ```rust
/// use typeset::*;
/// // Stays on one line even when narrower than its width.
/// assert_eq!(fix_pad(text("!"), text("condition")).compile().render(2, 5), "! condition");
/// ```
pub fn fix_pad(left: Layout, right: Layout) -> Layout {
    comp(left, right, Pad::Padded, Break::Fixed)
}

/// Unpadded composition that never breaks — `comp(left, right, Pad::Unpadded,
/// Break::Fixed)`. The way to attach punctuation to the literal beside it,
/// such as a separator to the item before it.
///
/// The fixed literals form one unbreakable run that takes the [`nest`]/
/// [`pack`] wrappers of its *first* literal. So fix a delimiter to what
/// precedes it, not to nested or packed content that follows it: `(` fixed
/// before `pack(args)` takes the first argument out of the pack. Compose an
/// opening delimiter with [`unpad`] instead.
///
/// ```rust
/// use typeset::*;
/// let doc = unpad(text("f("), fix_unpad(pad(text("x"), text("y")), text(")"))).compile();
/// assert_eq!(doc.render(2, 80), "f(x y)");
/// assert_eq!(doc.render(2, 3), "f(x\ny)");
/// ```
pub fn fix_unpad(left: Layout, right: Layout) -> Layout {
    comp(left, right, Pad::Unpadded, Break::Fixed)
}

// --- Joining: combine a collection --------------------------------------

/// Left-folds `layouts` with `combine`, returning [`null`] for an empty
/// collection and the sole element (untouched) for a singleton.
fn join(
    layouts: impl IntoIterator<Item = Layout>,
    combine: impl FnMut(Layout, Layout) -> Layout,
) -> Layout {
    layouts.into_iter().reduce(combine).unwrap_or_else(null)
}

/// Joins `layouts` with padded compositions: a space between neighbours that
/// share a line, nothing where a line breaks.
///
/// ```rust
/// use typeset::*;
/// let doc = join_with_spaces([text("Hello"), text("world")]).compile();
/// assert_eq!(doc.render(2, 80), "Hello world");
/// assert_eq!(doc.render(2, 5), "Hello\nworld");
/// ```
pub fn join_with_spaces(layouts: impl IntoIterator<Item = Layout>) -> Layout {
    join(layouts, pad)
}

/// Joins `layouts` as a comma-separated list: each comma is fixed to the
/// item before it, and the composition after it is padded and breakable.
///
/// ```rust
/// use typeset::*;
/// let doc = join_with_commas([text("x"), text("y"), text("z")]).compile();
/// assert_eq!(doc.render(2, 80), "x, y, z");
/// assert_eq!(doc.render(2, 3), "x,\ny,\nz");
/// ```
pub fn join_with_commas(layouts: impl IntoIterator<Item = Layout>) -> Layout {
    join(layouts, |acc, layout| {
        pad(fix_unpad(acc, text(",")), layout)
    })
}

/// Joins `layouts` with forced [`line()`] breaks — one element per line.
///
/// ```rust
/// use typeset::*;
/// let lines = join_with_lines([text("a;"), text("b;")]);
/// assert_eq!(lines.compile().render(2, 80), "a;\nb;");
/// ```
pub fn join_with_lines(layouts: impl IntoIterator<Item = Layout>) -> Layout {
    join(layouts, line)
}
