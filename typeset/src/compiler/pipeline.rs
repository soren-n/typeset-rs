//! Compilation pipeline: [`Layout`] → [`Doc`].
//!
//! This is the authoritative description of the pass order; the pass modules
//! and crate docs link here rather than repeating it.
//!
//! | Pass             | Lowers                    | Does |
//! |------------------|---------------------------|------|
//! | `resolve_breaks` | `Layout` → layout arena   | collapse broken sequences into hard lines |
//! | `serialize`      | layout arena → `SerialDoc` | flatten to leaf entries, computing scope open/close deltas |
//! | `split_lines`    | `SerialDoc` → `FixedDoc`  | split at hard lines, coalesce fixed-composition runs |
//! | `resolve_scopes` | `FixedDoc` → `RebuildDoc` | build, solve, and read back the grp/seq scope graph |
//! | `denull`         | `RebuildDoc` → `DenullDoc`| drop null/empty terms, strip term wrappers to prop lists |
//! | `normalize`      | `DenullDoc` → `DenullDoc` | eliminate trivial grp/seq, right-associate compositions |
//! | `rescope`        | `DenullDoc` → `Doc`       | factor shared nest/pack prefixes, build the heap `Doc` |
//!
//! Every representation, the input [`Layout`] included, is a flat structure —
//! postorder index arenas or plain vectors — so every pass is a loop (or an
//! explicit work-stack walk) and the whole pipeline runs in constant native
//! stack: no layout is too deep to compile, and depth shows up as O(depth)
//! heap instead. Every pass builds its accumulators in flat `Vec`-backed
//! arenas it owns and frees on return. The layout's text buffer is borrowed by
//! every representation down the pipeline; every intermediate drops as soon as
//! the next representation is built, so peak memory is a narrow window around
//! the largest pair of adjacent IRs rather than the sum of all of them. The
//! output [`Doc`] is a flat arena whose `Clone`/`Drop`/`Debug` are derived and
//! non-recursive by construction.
//!
//! [`compile`] is the sole entry point and is infallible.

use crate::compiler::{
    passes::{denull, normalize, rescope, resolve_breaks, resolve_scopes, serialize, split_lines},
    types::{Doc, Layout},
};

/// Compiles a layout into an optimized document.
///
/// Infallible: the pipeline is iterative, so no layout is too deep to compile
/// and there is no depth cap. Layout depth shows up as O(depth) heap, freed once
/// compilation returns.
///
/// # Examples
///
/// ```rust
/// use typeset::{compile, render, text};
///
/// let doc = compile(text("Hello, world!"));
/// assert_eq!(render(&doc, 2, 80), "Hello, world!");
/// ```
pub fn compile(layout: Layout) -> Doc {
    // The layout's text buffer is borrowed all the way down the pipeline; its
    // node arena is dead once the breaks are resolved.
    let Layout { nodes, text } = layout;
    let serial = {
        let resolved = resolve_breaks(&nodes);
        drop(nodes);
        serialize(&resolved, &text)
    };

    let denull_doc = {
        let line_doc = split_lines(&serial.entries);
        let scoped_doc = resolve_scopes(&line_doc, &serial.scopes);
        drop(line_doc);
        denull(&scoped_doc, &serial.paths)
    };
    drop(serial);

    let normalized_doc = normalize(denull_doc);
    rescope(normalized_doc)
}

impl Layout {
    /// Compiles this layout into a [`Doc`]; see [`compile`].
    pub fn compile(self) -> Doc {
        compile(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::constructors::*;
    use crate::compiler::render::render;
    use crate::compiler::types::{Break, Pad};

    #[test]
    fn test_compile_simple_text() {
        let doc = compile(text("hello"));
        assert_eq!(render(&doc, 2, 80), "hello");
    }

    #[test]
    fn test_compile_complex_layout() {
        let left = text("hello");
        let right = text("world");
        let layout = comp(left, right, Pad::Padded, Break::Breakable);
        let doc = compile(layout);
        assert_eq!(render(&doc, 2, 80), "hello world");
    }

    #[test]
    fn test_compile_nested_layout() {
        let inner = text("content");
        let nested = nest(inner);
        let grouped = grp(nested);
        let doc = compile(grouped);
        // `nest` indents its content by one tab (2 spaces here).
        assert_eq!(render(&doc, 2, 80), "  content");
    }

    #[test]
    fn test_render_compiled_doc() {
        let layout = text("test");
        let doc = compile(layout);
        let output = render(&doc, 4, 80);
        // Just ensure it doesn't panic - actual rendering logic tested elsewhere
        assert!(!output.is_empty());
    }

    // End-to-end deep-safety.
    //
    // The whole pipeline is iterative now — the compiler passes, the heap
    // conversion, the renderer, and dropping the resulting `Doc` all run with a
    // constant native stack. These build layouts far past the depth at which the
    // recursive tail used to abort (~1,000-2,000 on rendering), compile, render,
    // and let every intermediate drop.
    const DEEP: usize = 50_000;

    #[test]
    fn deep_nest_compiles_renders_and_drops() {
        let mut layout = text("x");
        for _ in 0..DEEP {
            layout = nest(layout);
        }
        let doc = compile(layout);
        let output = render(&doc, 2, 80);
        // Pure nesting introduces no line breaks; only leading indentation.
        assert!(!output.contains('\n'));
        assert!(output.ends_with('x'));
    }

    #[test]
    fn deep_comp_compiles_renders_and_drops() {
        // Left-nested compositions; a narrow width forces a break at each,
        // exercising the renderer's deep break path and the deep `Doc` spine.
        let mut layout = text("a");
        for _ in 0..DEEP {
            layout = comp(layout, text("b"), Pad::Padded, Break::Breakable);
        }
        let doc = compile(layout);
        let output = render(&doc, 2, 1);
        assert!(output.contains('\n'));
        assert!(output.ends_with('b'));
    }

    #[test]
    fn render_is_reusable() {
        let layout = comp(text("hello"), text("world"), Pad::Padded, Break::Breakable);
        let doc = compile(layout);
        // Borrowing renders the same document repeatedly without moving it.
        let a = render(&doc, 2, 5);
        let b = render(&doc, 2, 80);
        assert_eq!(b, "hello world");
        assert!(a.contains('\n'));
    }
}
