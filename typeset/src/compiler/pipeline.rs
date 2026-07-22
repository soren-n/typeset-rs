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
    passes::{broken, denull, fixed, flatten, normalize, rescope, serialize, structurize},
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

    // Flattening is the one step that walks the owning `Box` tree; every later
    // pass folds flat structures with plain loops. Text lives in the layout
    // arena and is borrowed all the way down the pipeline.
    let arena = flatten(layout);
    let edsl = broken(&arena);

    // serialize's persistent scope accumulators share structure, so it keeps
    // the pipeline's one bump arena; its terms are borrowed through every
    // later pass.
    let mem = Bump::new();
    let serial = serialize(&mem, &edsl);

    let fixed_doc = fixed(&serial);
    let rebuild_doc = structurize(&fixed_doc);
    let denull_doc = denull(&rebuild_doc);
    let normalized_doc = normalize(denull_doc);
    rescope(normalized_doc)
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
