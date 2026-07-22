//! Compilation pipeline: [`Layout`] → [`Doc`].
//!
//! The pipeline runs eight sequential passes, each lowering the tree through one
//! intermediate representation:
//!
//! ```text
//! Layout → Edsl → Serial → LinearDoc → FixedDoc → RebuildDoc →
//! DenullDoc → (normalize) DenullDoc → Doc
//! ```
//!
//! Each intermediate pass allocates its output in a fresh bump arena; the final
//! pass ([`rescope`](crate::compiler::passes::rescope)) builds the owned heap
//! [`Doc`] directly. The passes and renderer are iterative, and the output
//! [`Doc`] is a flat `Vec`-backed arena whose `Clone`/`Drop` are non-recursive
//! by construction, so the whole pipeline runs in constant native stack and deep
//! layouts never overflow it. Depth shows up as O(depth) heap instead.
//!
//! [`compile`] is the sole entry point and is infallible: the pipeline is
//! iterative, so no layout is too deep to compile and there is no depth cap.

use crate::compiler::{
    passes::{broken, denull, fixed, normalize, rescope, serialize, structurize},
    render::render_ref as render_ref_impl,
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
/// assert_eq!(render(doc, 2, 80), "Hello, world!");
/// ```
pub fn compile(layout: Box<Layout>) -> Box<Doc> {
    run_passes(layout)
}

/// Runs the eight-pass pipeline, lowering [`Layout`] to a heap [`Doc`].
///
/// Infallible and iterative. Each intermediate pass allocates its output in a
/// fresh bump arena, so every intermediate representation is freed once this
/// returns; the final pass builds the heap [`Doc`] directly.
fn run_passes(layout: Box<Layout>) -> Box<Doc> {
    use bumpalo::Bump;

    let mem1 = Bump::new();
    let edsl = broken(&mem1, layout);

    let mem2 = Bump::new();
    let serial = serialize(&mem2, edsl);

    // The remaining passes work on owned flat structures — no bumps needed.
    let fixed_doc = fixed(serial);
    let rebuild_doc = structurize(&fixed_doc);
    let denull_doc = denull(&rebuild_doc);
    let normalized_doc = normalize(denull_doc);
    rescope(normalized_doc)
}

/// Renders a compiled document to a formatted string, consuming it.
///
/// To render the same document more than once (e.g. at several widths) without
/// cloning it, use [`render_ref`] instead.
///
/// `tab` is the number of spaces per indentation level. `width` is the target
/// line width, counted in `char`s (not display columns — East Asian wide
/// characters and emoji count as one, so text using them renders wider than the
/// requested width). Use a very large width (e.g. 10000) to disable wrapping.
///
/// # Examples
///
/// ```rust
/// use typeset::{compile, render, text, comp, Pad, Break};
///
/// let doc = compile(comp(
///     text("hello"),
///     text("world"),
///     Pad::Padded, Break::Breakable,
/// ));
/// assert_eq!(render(doc, 2, 80), "hello world");
/// ```
pub fn render(doc: Box<Doc>, tab: usize, width: usize) -> String {
    render_ref_impl(&doc, tab, width)
}

/// Renders a compiled document by reference, without consuming it.
///
/// The borrowing counterpart to [`render()`]: because rendering only reads the
/// document, this renders the same [`Doc`] repeatedly (e.g. at several widths)
/// without cloning or recompiling it. See [`render()`] for the meaning of `tab`
/// and `width`.
///
/// # Examples
///
/// ```rust
/// use typeset::{compile, render_ref, text, comp, Pad, Break};
///
/// let doc = compile(comp(
///     text("hello"),
///     text("world"),
///     Pad::Padded, Break::Breakable,
/// ));
/// // Render at several widths without moving the document.
/// assert!(render_ref(&doc, 2, 5).contains('\n'));
/// assert_eq!(render_ref(&doc, 2, 80), "hello world");
/// ```
pub fn render_ref(doc: &Doc, tab: usize, width: usize) -> String {
    render_ref_impl(doc, tab, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::constructors::*;
    use crate::compiler::types::{Break, Pad};

    #[test]
    fn test_compile_simple_text() {
        let doc = compile(text("hello"));
        assert_eq!(render(doc, 2, 80), "hello");
    }

    #[test]
    fn test_compile_complex_layout() {
        let left = text("hello");
        let right = text("world");
        let layout = comp(left, right, Pad::Padded, Break::Breakable);
        let doc = compile(layout);
        assert_eq!(render(doc, 2, 80), "hello world");
    }

    #[test]
    fn test_compile_nested_layout() {
        let inner = text("content");
        let nested = nest(inner);
        let grouped = grp(nested);
        let doc = compile(grouped);
        // `nest` indents its content by one tab (2 spaces here).
        assert_eq!(render(doc, 2, 80), "  content");
    }

    #[test]
    fn test_render_compiled_doc() {
        let layout = text("test");
        let doc = compile(layout);
        let output = render(doc, 4, 80);
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
        let output = render(doc, 2, 80);
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
        let output = render(doc, 2, 1);
        assert!(output.contains('\n'));
        assert!(output.ends_with('b'));
    }

    #[test]
    fn render_ref_matches_render_and_is_reusable() {
        let layout = comp(text("hello"), text("world"), Pad::Padded, Break::Breakable);
        let doc = compile(layout);
        // Borrowing renders the same document repeatedly without moving it.
        let a = render_ref(&doc, 2, 5);
        let b = render_ref(&doc, 2, 80);
        assert_eq!(b, "hello world");
        assert!(a.contains('\n'));
        // Consuming render produces identical output to the borrowing variant.
        assert_eq!(render(doc, 2, 80), b);
    }
}
