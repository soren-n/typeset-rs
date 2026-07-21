//! Compilation pipeline: [`Layout`] → [`Doc`].
//!
//! The pipeline runs ten sequential passes, each lowering the tree through one
//! intermediate representation:
//!
//! ```text
//! Layout → Edsl → Serial → LinearDoc → FixedDoc → RebuildDoc →
//! DenullDoc → IdentitiesDoc → ReassociateDoc → FinalDoc → Doc
//! ```
//!
//! Each pass allocates its output in a fresh bump arena; the final pass moves
//! the result to the heap [`Doc`]. The whole pipeline — passes, heap conversion,
//! renderer, and dropping the [`Doc`] — is iterative, so it runs in constant
//! native stack and deep layouts never overflow it. Depth shows up as O(depth)
//! heap instead.
//!
//! Two entry points: [`compile`] panics on internal error (fast path);
//! [`compile_safe`]/[`compile_safe_with_depth`] return a [`Result`] and reject
//! layouts deeper than a configurable bound.

use crate::compiler::{
    error::CompilerError,
    passes::{
        broken, denull, fixed, identities, linearize, move_to_heap, reassociate, rescope,
        serialize, structurize,
    },
    render::{render as render_doc, render_ref as render_ref_doc},
    types::{Doc, Layout},
};

/// Compiles a layout into an optimized document, panicking on internal error.
///
/// The fast path: it runs [`compile_safe`] and unwraps. The pipeline is
/// iterative, so deep layouts do not overflow the stack; if layout depth is
/// untrusted, use [`compile_safe_with_depth`] to bound it up front.
///
/// # Examples
///
/// ```rust
/// use typeset::{compile, render, text};
///
/// let doc = compile(text("Hello, world!".to_string()));
/// assert_eq!(render(doc, 2, 80), "Hello, world!");
/// ```
pub fn compile(layout: Box<Layout>) -> Box<Doc> {
    match compile_safe(layout) {
        Ok(doc) => doc,
        Err(e) => panic!("Compilation failed: {:?}", e),
    }
}

/// Compiles a layout, returning a [`Result`] and rejecting layouts deeper than
/// the default bound of 10,000.
///
/// The depth bound is a resource limit, not a stack-safety guard (the pipeline
/// is iterative). Use [`compile_safe_with_depth`] to choose a different bound.
///
/// # Examples
///
/// ```rust
/// use typeset::{compile_safe, render, text};
///
/// let doc = compile_safe(text("Hello, world!".to_string())).unwrap();
/// assert_eq!(render(doc, 2, 80), "Hello, world!");
/// ```
pub fn compile_safe(layout: Box<Layout>) -> Result<Box<Doc>, CompilerError> {
    compile_safe_with_depth(layout, 10000)
}

/// Maximum nesting depth of a layout tree.
///
/// Walks iteratively with an explicit stack: a recursive walk would overflow on
/// exactly the deep inputs this exists to reject.
fn _measure_depth(layout: &Layout) -> usize {
    let mut deepest = 0usize;
    let mut stack: Vec<(&Layout, usize)> = vec![(layout, 1)];
    while let Some((node, depth)) = stack.pop() {
        deepest = deepest.max(depth);
        match node {
            Layout::Null | Layout::Text(_) => {}
            Layout::Fix(inner)
            | Layout::Grp(inner)
            | Layout::Seq(inner)
            | Layout::Nest(inner)
            | Layout::Pack(inner) => stack.push((inner, depth + 1)),
            Layout::Line(left, right) => {
                stack.push((left, depth + 1));
                stack.push((right, depth + 1));
            }
            Layout::Comp(left, right, _) => {
                stack.push((left, depth + 1));
                stack.push((right, depth + 1));
            }
        }
    }
    deepest
}

/// Compiles a layout with a custom depth bound.
///
/// Layouts deeper than `max_depth` are rejected with
/// [`CompilerError::DepthLimitExceeded`] before compiling. Because the pipeline
/// is iterative, this is a policy/resource bound (capping the O(depth) heap an
/// untrusted layout can allocate), not a stack-safety guard — pick it from your
/// memory budget. `max_depth` must be greater than 0, else
/// [`CompilerError::InvalidInput`] is returned.
///
/// # Examples
///
/// ```rust
/// use typeset::{compile_safe_with_depth, render, text};
///
/// let doc = compile_safe_with_depth(text("content".to_string()), 1000).unwrap();
/// assert_eq!(render(doc, 2, 80), "content");
/// ```
pub fn compile_safe_with_depth(
    layout: Box<Layout>,
    max_depth: usize,
) -> Result<Box<Doc>, CompilerError> {
    if max_depth == 0 {
        return Err(CompilerError::InvalidInput(
            "max_depth must be greater than 0".to_string(),
        ));
    }

    // Reject over-deep layouts before compiling. The pipeline is fully iterative
    // (passes, heap conversion, renderer, and Doc drop), so this is a resource
    // bound rather than a stack-safety guard: it caps the O(depth) heap that an
    // untrusted layout can allocate. The walk itself is iterative so measuring a
    // deep layout cannot overflow.
    let depth = _measure_depth(&layout);
    if depth > max_depth {
        return Err(CompilerError::DepthLimitExceeded { depth, max_depth });
    }

    use bumpalo::Bump;

    // A separate bump arena per pass, so each intermediate representation is
    // freed once the pipeline returns.
    let mem1 = Bump::new();
    let edsl = broken(&mem1, layout);

    let mem2 = Bump::new();
    let serial = serialize(&mem2, edsl);

    let mem3 = Bump::new();
    let linear_doc = linearize(&mem3, serial);

    let mem4 = Bump::new();
    let fixed_doc = fixed(&mem4, linear_doc);

    let mem5 = Bump::new();
    let rebuild_doc = structurize(&mem5, fixed_doc);

    let mem6 = Bump::new();
    let denull_doc = denull(&mem6, rebuild_doc);

    let mem7 = Bump::new();
    let identities_doc = identities(&mem7, denull_doc);

    let mem8 = Bump::new();
    let reassociate_doc = reassociate(&mem8, identities_doc);

    let mem9 = Bump::new();
    let final_doc = rescope(&mem9, reassociate_doc);

    // Pass 10: FinalDoc → Doc (move to heap; does not use an arena).
    Ok(move_to_heap(final_doc))
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
/// use typeset::{compile, render, text, comp};
///
/// let doc = compile(comp(
///     text("hello".to_string()),
///     text("world".to_string()),
///     true, false,
/// ));
/// assert_eq!(render(doc, 2, 80), "hello world");
/// ```
pub fn render(doc: Box<Doc>, tab: usize, width: usize) -> String {
    render_doc(doc, tab, width)
}

/// Renders a compiled document by reference, without consuming it.
///
/// The borrowing counterpart to [`render`]: because rendering only reads the
/// document, this renders the same [`Doc`] repeatedly (e.g. at several widths)
/// without cloning or recompiling it. See [`render`] for the meaning of `tab`
/// and `width`.
///
/// # Examples
///
/// ```rust
/// use typeset::{compile, render_ref, text, comp};
///
/// let doc = compile(comp(
///     text("hello".to_string()),
///     text("world".to_string()),
///     true, false,
/// ));
/// // Render at several widths without moving the document.
/// assert!(render_ref(&doc, 2, 5).contains('\n'));
/// assert_eq!(render_ref(&doc, 2, 80), "hello world");
/// ```
pub fn render_ref(doc: &Doc, tab: usize, width: usize) -> String {
    render_ref_doc(doc, tab, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::constructors::*;

    #[test]
    fn test_compile_simple_text() {
        let layout = text("hello".to_string());
        let result = compile_safe(layout);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compile_safe_with_zero_depth() {
        let layout = text("hello".to_string());
        let result = compile_safe_with_depth(layout, 0);
        assert!(matches!(result, Err(CompilerError::InvalidInput(_))));
    }

    #[test]
    fn test_compile_complex_layout() {
        let left = text("hello".to_string());
        let right = text("world".to_string());
        let layout = comp(left, right, true, false);
        let result = compile_safe(layout);
        assert!(result.is_ok());
    }

    #[test]
    fn test_compile_nested_layout() {
        let inner = text("content".to_string());
        let nested = nest(inner);
        let grouped = grp(nested);
        let result = compile_safe(grouped);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_compiled_doc() {
        let layout = text("test".to_string());
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
    // recursive tail used to abort (~1,000-2,000 on rendering), compile with a
    // limit above that depth, render, and let every intermediate drop.
    const DEEP: usize = 50_000;

    #[test]
    fn deep_nest_compiles_renders_and_drops() {
        let mut layout = text("x".to_string());
        for _ in 0..DEEP {
            layout = nest(layout);
        }
        let doc = compile_safe_with_depth(layout, DEEP * 2).expect("deep nest should compile");
        let output = render(doc, 2, 80);
        // Pure nesting introduces no line breaks; only leading indentation.
        assert!(!output.contains('\n'));
        assert!(output.ends_with('x'));
    }

    #[test]
    fn deep_comp_compiles_renders_and_drops() {
        // Left-nested compositions; a narrow width forces a break at each,
        // exercising the renderer's deep break path and the deep `Doc` spine.
        let mut layout = text("a".to_string());
        for _ in 0..DEEP {
            layout = comp(layout, text("b".to_string()), true, false);
        }
        let doc = compile_safe_with_depth(layout, DEEP * 2).expect("deep comp should compile");
        let output = render(doc, 2, 1);
        assert!(output.contains('\n'));
        assert!(output.ends_with('b'));
    }

    #[test]
    fn render_ref_matches_render_and_is_reusable() {
        let layout = comp(
            text("hello".to_string()),
            text("world".to_string()),
            true,
            false,
        );
        let doc = compile(layout);
        // Borrowing renders the same document repeatedly without moving it.
        let a = render_ref(&doc, 2, 5);
        let b = render_ref(&doc, 2, 80);
        assert_eq!(b, "hello world");
        assert!(a.contains('\n'));
        // Consuming render produces identical output to the borrowing variant.
        assert_eq!(render(doc, 2, 80), b);
    }

    #[test]
    fn deep_layout_beyond_limit_is_rejected() {
        // The depth check is now a policy bound rather than a stack-safety
        // guard, but it still rejects layouts deeper than the configured limit.
        let mut layout = text("x".to_string());
        for _ in 0..1000 {
            layout = nest(layout);
        }
        let result = compile_safe_with_depth(layout, 100);
        assert!(matches!(
            result,
            Err(CompilerError::DepthLimitExceeded { .. })
        ));
    }
}
