//! Constructors for joining a collection of layouts with a separator: a custom
//! one, spaces, commas, or line breaks.

use super::basic::null;
use super::composition::{comp, line};
use super::text_utils::{comma, space};
use crate::compiler::types::Layout;

/// Joins `layouts` with `separator` between each pair, via unpadded
/// compositions (the separator supplies its own spacing). Returns [`null`] for
/// an empty vector and the sole element for a singleton.
///
/// ```rust
/// use typeset::*;
/// let joined = join_with(vec![text("a"), text("b")], comp(comma(), space(), false, false));
/// assert_eq!(format_layout(joined, 2, 80), "a, b");
/// ```
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

/// Joins `layouts` with single spaces — `join_with(layouts, space())`.
///
/// ```rust
/// use typeset::*;
/// let sentence = join_with_spaces(vec![text("Hello"), text("world")]);
/// assert_eq!(format_layout(sentence, 2, 80), "Hello world");
/// ```
pub fn join_with_spaces(layouts: Vec<Box<Layout>>) -> Box<Layout> {
    join_with(layouts, space())
}

/// Joins `layouts` with `", "` separators — the standard comma-separated list.
///
/// ```rust
/// use typeset::*;
/// let params = join_with_commas(vec![text("x"), text("y"), text("z")]);
/// assert_eq!(format_layout(params, 2, 80), "x, y, z");
/// ```
pub fn join_with_commas(layouts: Vec<Box<Layout>>) -> Box<Layout> {
    join_with(layouts, comp(comma(), space(), false, false))
}

/// Joins `layouts` with forced [`line()`] breaks — one element per line.
///
/// ```rust
/// use typeset::*;
/// let lines = join_with_lines(vec![text("a;"), text("b;")]);
/// assert_eq!(format_layout(lines, 2, 80), "a;\nb;");
/// ```
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
