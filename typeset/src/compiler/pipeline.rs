//! Compilation Pipeline for Pretty Printing Layouts
//!
//! This module implements the complete 10-pass compilation pipeline that transforms
//! high-level [`Layout`] trees into optimized [`Doc`] structures ready for rendering.
//! It provides both fast compilation paths and safe variants with comprehensive error handling.
//!
//! # Pipeline Architecture
//!
//! The compilation pipeline consists of 10 sequential passes, each transforming the
//! input through a series of intermediate representations:
//!
//! ```text
//! Layout → Edsl → Serial → LinearDoc → FixedDoc → RebuildDoc →
//! DenullDoc → IdentitiesDoc → ReassociateDoc → FinalDoc → Doc
//! ```
//!
//! Each pass serves a specific purpose:
//!
//! 1. **Layout → Edsl**: Normalize layout structure and resolve control constructs
//! 2. **Edsl → Serial**: Serialize nested structures into linear sequences
//! 3. **Serial → LinearDoc**: Convert to document format with layout decisions
//! 4. **LinearDoc → FixedDoc**: Fix positions and resolve line breaking
//! 5. **FixedDoc → RebuildDoc**: Rebuild optimized document structure
//! 6. **RebuildDoc → DenullDoc**: Remove null elements and optimize structure
//! 7. **DenullDoc → IdentitiesDoc**: Apply identity transformations and simplifications
//! 8. **IdentitiesDoc → ReassociateDoc**: Reassociate operations for better layout
//! 9. **ReassociateDoc → FinalDoc**: Final optimizations and cleanup
//! 10. **FinalDoc → Doc**: Move from bump allocation to heap allocation
//!
//! # Memory Management
//!
//! The pipeline uses efficient bump allocation during compilation passes to minimize
//! memory overhead and garbage collection pressure. Each pass uses a separate bump
//! allocator to avoid lifetime conflicts, with the final pass moving the result
//! to heap allocation for long-term storage.
//!
//! # Performance Characteristics
//!
//! - **Time Complexity**: O(n) where n is the number of layout nodes
//! - **Space Complexity**: O(n) peak memory during compilation, O(m) after where m ≤ n
//! - **Stack Usage**: The whole pipeline is iterative — the compiler passes, the
//!   heap conversion, the renderer, and dropping the resulting [`Doc`] all run
//!   with a constant native stack. Layout depth shows up as heap usage (the
//!   explicit frame stacks and the [`Doc`] itself), not native-stack depth
//! - **Memory Allocation**: 10 bump allocators during compilation, final heap allocation
//!
//! # Error Handling
//!
//! The pipeline provides two compilation strategies:
//!
//! - **Fast path** ([`compile()`]): Panics on errors for maximum performance
//! - **Safe path** ([`compile_safe()`], [`compile_safe_with_depth()`]): Returns [`Result`] with error details
//!
//! # Usage Patterns
//!
//! ## Basic Compilation
//!
//! ```rust
//! use typeset::{compile, render, text};
//!
//! let layout = text("Hello, world!".to_string());
//! let doc = compile(layout);
//! let output = render(doc, 2, 80);
//! ```
//!
//! ## Safe Compilation with Error Handling
//!
//! ```rust
//! use typeset::{compile_safe, render, text, comp, nest};
//!
//! // Build a complex layout that might fail
//! let layout = comp(
//!     text("function".to_string()),
//!     nest(text("body".to_string())),
//!     true, false
//! );
//!
//! match compile_safe(layout) {
//!     Ok(doc) => {
//!         let output = render(doc, 4, 120);
//!         println!("Compiled successfully: {}", output);
//!     }
//!     Err(e) => {
//!         eprintln!("Compilation failed: {}", e);
//!     }
//! }
//! ```
//!
//! ## Custom Recursion Limits
//!
//! ```rust
//! use typeset::{compile_safe_with_depth, text};
//!
//! let layout = text("content".to_string());
//!
//! // Use custom stack depth limit for deeply nested layouts
//! match compile_safe_with_depth(layout, 20000) {
//!     Ok(doc) => println!("Compiled with extended stack limit"),
//!     Err(e) => eprintln!("Failed even with extended limit: {}", e),
//! }
//! ```

use crate::compiler::{
    error::CompilerError,
    passes::{
        broken, denull, fixed, identities, linearize, move_to_heap, reassociate, rescope,
        serialize, structurize,
    },
    render::{render as render_doc, render_ref as render_ref_doc},
    types::{Doc, Layout},
};

/// Compiles a layout into an optimized document (fast path)
///
/// This is the main entry point for high-performance compilation. It executes all 10 compiler
/// passes using efficient bump allocation and panics on errors for maximum speed. Use this
/// function when you're confident in your layout structure and want minimal overhead.
///
/// The compilation process transforms the input [`Layout`] through multiple intermediate
/// representations, applying optimizations at each step to produce a [`Doc`] ready for
/// efficient rendering.
///
/// # Arguments
///
/// * `layout` - The input layout tree to compile. Must be a valid layout structure
///   created using constructor functions like [`text`](crate::text), [`comp`](crate::comp), etc.
///
/// # Returns
///
/// A boxed [`Doc`] containing the optimized document structure. The document can be
/// rendered multiple times with different parameters without recompilation.
///
/// # Panics
///
/// This function panics on internal compiler errors (unexpected states during
/// pass execution).
///
/// # Stack Safety
///
/// The whole pipeline is iterative — the compiler passes, the heap conversion,
/// the renderer, and dropping the resulting [`Doc`] all run with a constant
/// native stack, so deep layouts do not overflow the stack. Depth translates
/// into heap usage (explicit frame stacks plus the [`Doc`]), which is O(depth):
/// a pathologically deep layout can exhaust memory, but it will not SIGABRT on
/// stack overflow. If layout depth is untrusted, prefer
/// [`compile_safe_with_depth()`] to bound it up front.
///
/// # Performance Notes
///
/// - **Time**: O(n) where n is the number of layout nodes
/// - **Memory**: Uses 10 temporary bump allocators during compilation
/// - **Stack**: Constant native stack throughout (iterative passes, heap
///   conversion, renderer, and [`Doc`] drop)
/// - **Optimal for**: Production code with validated layouts where panics are acceptable
///
/// # Examples
///
/// ## Basic Text Layout
///
/// ```rust
/// use typeset::{compile, render, text};
///
/// let layout = text("Hello, world!".to_string());
/// let doc = compile(layout);
/// let output = render(doc, 2, 80);
/// assert_eq!(output, "Hello, world!");
/// ```
///
/// ## Complex Nested Layout
///
/// ```rust
/// use typeset::{compile, render, text, comp, nest, grp};
///
/// // Create a function definition layout
/// let layout = grp(comp(
///     text("function".to_string()),
///     nest(comp(
///         text("calculateSum(a, b)".to_string()),
///         comp(
///             text("{".to_string()),
///             nest(text("return a + b;".to_string())),
///             true, false
///         ),
///         false, false
///     )),
///     true, false
/// ));
///
/// let doc = compile(layout);
/// let output = render(doc, 4, 40);
/// // Will produce properly formatted function with indentation
/// ```
///
/// ## Performance-Critical Batch Compilation
///
/// ```rust
/// use typeset::{compile, render, text};
///
/// // Compile multiple layouts efficiently
/// let layouts = vec![
///     text("item1".to_string()),
///     text("item2".to_string()),
///     text("item3".to_string()),
/// ];
///
/// let docs: Vec<_> = layouts.into_iter()
///     .map(|layout| compile(layout))
///     .collect();
///
/// // Render all with consistent formatting
/// for doc in docs {
///     let output = render(doc, 2, 80);
///     println!("{}", output);
/// }
/// ```
///
/// # See Also
///
/// - [`compile_safe()`] - Safe compilation with error handling
/// - [`compile_safe_with_depth()`] - Safe compilation with custom recursion limits
/// - [`render()`] - Convert compiled documents to strings
/// - [Layout constructors](crate#layout-constructors) - Functions for building layouts
pub fn compile(layout: Box<Layout>) -> Box<Doc> {
    match compile_safe(layout) {
        Ok(doc) => doc,
        Err(e) => panic!("Compilation failed: {:?}", e),
    }
}

/// Compiles a layout into an optimized document with error handling (safe path)
///
/// This function provides safe compilation with comprehensive error handling instead of panics.
/// It uses the same 10-pass compilation pipeline as [`compile()`] but returns a [`Result`] to
/// handle errors gracefully. The function applies a default maximum nesting depth
/// of 10,000.
///
/// The whole pipeline is iterative — the passes, the heap conversion, the
/// renderer, and dropping the [`Doc`] all use a constant native stack — so this
/// limit is a policy/resource bound (guarding against pathological memory and
/// time on adversarial input) rather than a stack-overflow guard. The 10,000
/// default is a reasonable ceiling for typical use; raise or lower it with
/// [`compile_safe_with_depth()`] to suit your own memory budget.
///
/// # Arguments
///
/// * `layout` - The input layout tree to compile. Can be any layout created with
///   constructor functions, including deeply nested or potentially problematic structures.
///
/// # Returns
///
/// * `Ok(Box<Doc>)` - Successfully compiled document ready for rendering
/// * `Err(CompilerError)` - Compilation failed with detailed error information
///
/// # Error Conditions
///
/// This function can return the following errors:
///
/// - [`CompilerError::StackOverflow`] - Layout nesting exceeds the default 10,000 level limit
///
/// [`CompilerError::AllocationFailed`] and [`CompilerError::InvalidInput`] are
/// never returned by this function.
///
/// # Performance Notes
///
/// - **Time**: O(n) where n is the number of layout nodes (same as [`compile()`])
/// - **Memory**: Uses 10 temporary bump allocators during compilation
/// - **Overhead**: Minimal additional cost compared to [`compile()`] (error checking only)
/// - **Stack Protection**: Includes recursion depth tracking to prevent stack overflow
///
/// # Examples
///
/// ## Basic Error Handling
///
/// ```rust
/// use typeset::{compile_safe, render, text};
///
/// let layout = text("Hello, world!".to_string());
/// match compile_safe(layout) {
///     Ok(doc) => {
///         let output = render(doc, 2, 80);
///         println!("Success: {}", output);
///     }
///     Err(e) => {
///         eprintln!("Compilation failed: {}", e);
///     }
/// }
/// ```
///
/// ## Handling Complex Layout Compilation
///
/// ```rust
/// use typeset::{compile_safe, render, text, comp, nest, grp, seq, join_with_lines};
///
/// // Build a potentially complex layout
/// let items: Vec<_> = (0..100)
///     .map(|i| text(format!("item_{}", i)))
///     .collect();
///
/// let layout = seq(join_with_lines(items));
///
/// match compile_safe(layout) {
///     Ok(doc) => {
///         let output = render(doc, 4, 80);
///         println!("Compiled {} items successfully", 100);
///     }
///     Err(e) => {
///         eprintln!("Failed to compile large layout: {}", e);
///         // Handle error appropriately - maybe reduce complexity
///     }
/// }
/// ```
///
/// ## User Input Compilation (Untrusted Layouts)
///
/// ```rust
/// use typeset::{compile_safe, render, text, CompilerError};
///
/// fn compile_user_layout(user_content: String) -> Result<String, String> {
///     let layout = text(user_content);
///     
///     match compile_safe(layout) {
///         Ok(doc) => {
///             let output = render(doc, 2, 80);
///             Ok(output)
///         }
///         Err(CompilerError::StackOverflow { depth, max_depth }) => {
///             Err(format!("Layout too complex: {} levels (max: {})", depth, max_depth))
///         }
///         Err(CompilerError::AllocationFailed(msg)) => {
///             Err(format!("Out of memory: {}", msg))
///         }
///         Err(CompilerError::InvalidInput(msg)) => {
///             Err(format!("Invalid layout: {}", msg))
///         }
///     }
/// }
/// ```
///
/// ## Batch Compilation with Error Recovery
///
/// ```rust
/// use typeset::{compile_safe, render, text};
///
/// fn compile_multiple_layouts(contents: Vec<String>) -> Vec<Result<String, String>> {
///     contents.into_iter()
///         .map(|content| {
///             let layout = text(content);
///             match compile_safe(layout) {
///                 Ok(doc) => Ok(render(doc, 2, 80)),
///                 Err(e) => Err(e.to_string()),
///             }
///         })
///         .collect()
/// }
/// ```
///
/// # See Also
///
/// - [`compile()`] - Fast compilation that panics on errors
/// - [`compile_safe_with_depth()`] - Safe compilation with custom recursion limits
/// - [`CompilerError`] - Detailed error types and descriptions
/// - [`render()`] - Convert compiled documents to strings
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

/// Compiles a layout with custom recursion depth limit and error handling (configurable safe path)
///
/// This function provides the most flexible compilation approach, allowing fine-grained control
/// over the maximum accepted layout depth. It's useful when you want to bound the
/// memory and time an adversarially deep layout can consume.
///
/// The function uses the same 10-pass compilation pipeline as [`compile()`] and [`compile_safe()`]
/// but allows customization of the depth threshold at which a layout is rejected.
///
/// # Arguments
///
/// * `layout` - The input layout tree to compile. Can handle arbitrarily complex structures
///   within the depth limit.
/// * `max_depth` - Maximum layout nesting depth accepted. Must be greater than 0.
///   Layouts deeper than this are rejected with [`CompilerError::StackOverflow`]
///   before compilation begins.
///
///   The whole pipeline is iterative, so this is a policy/resource bound, not a
///   stack-safety guard: depth translates into O(depth) heap usage (the explicit
///   frame stacks and the [`Doc`]), and the limit caps how much an untrusted
///   layout can allocate. Choose it from your memory budget; a deep layout does
///   not overflow the native stack regardless of the value.
///
/// # Returns
///
/// * `Ok(Box<Doc>)` - Successfully compiled document ready for rendering
/// * `Err(CompilerError)` - Compilation failed with detailed error information
///
/// # Error Conditions
///
/// This function can return several types of errors:
///
/// - [`CompilerError::InvalidInput`] - If `max_depth` is 0
/// - [`CompilerError::StackOverflow`] - If layout nesting exceeds `max_depth`
///
/// [`CompilerError::AllocationFailed`] is never returned; bump allocation
/// failure aborts rather than reporting an error.
///
/// # Performance Notes
///
/// - **Time**: O(n) where n is the number of layout nodes
/// - **Memory**: Uses 10 temporary bump allocators during compilation
/// - **Stack Usage**: Constant native stack throughout (iterative passes, heap
///   conversion, renderer, and [`Doc`] drop)
/// - **Depth Check**: One iterative O(n) walk before compiling
/// - **Optimal Use**: When layout depth is not under your control
///
/// # Choosing a Depth Limit
///
/// The limit is a resource bound, not a stack-safety guard — nothing in the
/// pipeline overflows the native stack. Pick it from how much memory and time
/// you are willing to spend on a single layout: peak usage grows roughly O(depth)
/// (frame stacks plus the [`Doc`]). The default used by [`compile_safe()`] is
/// 10,000, a reasonable ceiling for typical use.
///
/// # Examples
///
/// ## Conservative Compilation for Web Services
///
/// ```rust
/// use typeset::{compile_safe_with_depth, render, text};
///
/// fn compile_user_input(content: String) -> Result<String, String> {
///     let layout = text(content);
///     
///     // Use conservative depth limit for untrusted input
///     match compile_safe_with_depth(layout, 1000) {
///         Ok(doc) => Ok(render(doc, 2, 80)),
///         Err(e) => Err(format!("Compilation failed: {}", e)),
///     }
/// }
/// ```
///
/// ## High-Performance Compilation for Code Generation
///
/// ```rust
/// use typeset::{compile_safe_with_depth, render, text, comp, nest};
///
/// fn compile_syntax_tree(depth: usize) -> Result<String, String> {
///     // Build a deeply nested layout representing a syntax tree
///     let mut layout = text("root".to_string());
///     for i in 0..depth {
///         layout = nest(comp(
///             layout,
///             text(format!("node_{}", i)),
///             false, false
///         ));
///     }
///
///     // Use high depth limit for known deep structures
///     match compile_safe_with_depth(layout, 25000) {
///         Ok(doc) => Ok(render(doc, 4, 120)),
///         Err(e) => Err(format!("Failed to compile deep tree: {}", e)),
///     }
/// }
/// ```
///
/// ## Resource-Constrained Environment
///
/// ```rust
/// use typeset::{compile_safe_with_depth, render, text, CompilerError};
///
/// fn compile_with_limited_stack(layout: Box<typeset::Layout>) -> Result<String, String> {
///     // Use very conservative limit for embedded systems
///     match compile_safe_with_depth(layout, 500) {
///         Ok(doc) => Ok(render(doc, 2, 40)),
///         Err(CompilerError::StackOverflow { depth, max_depth }) => {
///             Err(format!(
///                 "Layout too deep for this system: {} levels (max: {})",
///                 depth, max_depth
///             ))
///         }
///         Err(e) => Err(format!("Compilation error: {}", e)),
///     }
/// }
/// ```
///
/// ## Adaptive Depth Strategy
///
/// ```rust
/// use typeset::{compile_safe_with_depth, render, text, CompilerError};
///
/// fn compile_with_fallback(layout: Box<typeset::Layout>) -> Result<String, String> {
///     // Try aggressive first, fall back to conservative
///     let depths = [20000, 10000, 5000, 1000];
///     
///     for &depth in &depths {
///         match compile_safe_with_depth(layout.clone(), depth) {
///             Ok(doc) => return Ok(render(doc, 2, 80)),
///             Err(CompilerError::StackOverflow { .. }) => continue,
///             Err(e) => return Err(format!("Non-depth error: {}", e)),
///         }
///     }
///     
///     Err("Layout too complex even with minimum depth limit".to_string())
/// }
/// ```
///
/// ## Parameter Validation
///
/// ```rust
/// use typeset::{compile_safe_with_depth, text, CompilerError};
///
/// let layout = text("test".to_string());
///
/// // This will fail with InvalidInput error
/// match compile_safe_with_depth(layout, 0) {
///     Err(CompilerError::InvalidInput(msg)) => {
///         println!("Expected error: {}", msg);
///     }
///     _ => panic!("Should have failed with InvalidInput"),
/// }
/// ```
///
/// # See Also
///
/// - [`compile()`] - Fast compilation that panics on errors
/// - [`compile_safe()`] - Safe compilation with default 10,000 depth limit
/// - [`CompilerError`] - Detailed error types and descriptions
/// - [`render()`] - Convert compiled documents to strings
pub fn compile_safe_with_depth(
    layout: Box<Layout>,
    max_depth: usize,
) -> Result<Box<Doc>, CompilerError> {
    // Validate recursion depth parameter
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
        return Err(CompilerError::StackOverflow { depth, max_depth });
    }

    use bumpalo::Bump;

    // Use separate bump allocators for each pass to avoid lifetime issues
    // This mimics the original implementation pattern

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

    // Pass 10: FinalDoc → Doc (move to heap)
    // This pass doesn't use the allocator as it moves data to heap
    let heap_doc = move_to_heap(final_doc);

    Ok(heap_doc)
}

/// Renders a compiled document to a formatted string
///
/// This is the final step in the pretty printing process. It takes a compiled [`Doc`] structure
/// and produces the final formatted text output according to the specified formatting parameters.
/// The rendering process is deterministic.
///
/// This variant consumes the document. To render the same document more than
/// once (for example at several widths) without cloning it, use [`render_ref()`]
/// instead.
///
/// The renderer implements sophisticated line breaking algorithms that respect the layout
/// decisions made during compilation while applying the final formatting constraints.
///
/// # Arguments
///
/// * `doc` - The compiled document to render. Must be a [`Doc`] produced by one of the
///   compilation functions ([`compile()`], [`compile_safe()`], or [`compile_safe_with_depth()`]).
/// * `tab` - Number of spaces per indentation level. Common values:
///   - **2**: Compact style, common in web development
///   - **4**: Standard style, widely used in many languages  
///   - **8**: Traditional tab width, used in system programming
/// * `width` - Maximum line width for line breaking decisions, counted in
///   **characters**. The renderer will attempt to keep lines within this limit
///   while respecting the layout structure. Common values:
///   - **40-60**: Narrow columns for documentation or mobile display
///   - **80**: Traditional terminal width, widely used standard
///   - **100-120**: Modern wide displays, good for code review
///   - **Unlimited**: Use very large values (e.g., 10000) to disable line wrapping
///
///   Width counts `char`s, not display columns. East Asian wide characters and
///   emoji occupy two terminal columns but count as one here, so text using them
///   renders wider than the requested width.
///
/// # Returns
///
/// A [`String`] containing the formatted text output with appropriate line breaks,
/// indentation, and spacing applied according to the layout structure and formatting parameters.
///
/// # Performance Notes
///
/// - **Time**: O(n) where n is the size of the final output text
/// - **Memory**: O(n) for the output string allocation
/// - **Reusability**: Render the same document multiple times via [`render_ref()`]
/// - **No Side Effects**: Pure function with no global state or side effects
///
/// # Formatting Behavior
///
/// The renderer applies several formatting rules:
///
/// - **Indentation**: Applied according to `nest` and `pack` constructs in the original layout
/// - **Line Breaking**: Respects `grp`, `seq`, and composition breaking rules from compilation
/// - **Spacing**: Handles padding and spacing between text elements
/// - **Width Constraints**: Attempts to fit content within the specified `width` when possible
///
/// # Examples
///
/// ## Basic Text Rendering
///
/// ```rust
/// use typeset::{compile, render, text};
///
/// let layout = text("Hello, world!".to_string());
/// let doc = compile(layout);
///
/// // Render with standard settings
/// let output = render(doc, 4, 80);
/// assert_eq!(output, "Hello, world!");
/// ```
///
/// ## Comparing Different Tab Widths
///
/// ```rust
/// use typeset::{compile, render_ref, text, nest};
///
/// let layout = nest(text("indented content".to_string()));
/// let doc = compile(layout);
///
/// // Compare different indentation widths by borrowing the same document.
/// let narrow = render_ref(&doc, 2, 80);
/// let standard = render_ref(&doc, 4, 80);
/// let wide = render_ref(&doc, 8, 80);
///
/// // Each will have different indentation spacing
/// ```
///
/// ## Width Constraint Comparison
///
/// ```rust
/// use typeset::{compile, render_ref, text, comp};
///
/// let layout = comp(
///     text("This is a long line that might need".to_string()),
///     text("to break depending on width constraints".to_string()),
///     true, false
/// );
/// let doc = compile(layout);
///
/// // Narrow width forces breaking
/// let narrow = render_ref(&doc, 2, 40);
///
/// // Wide width allows single line
/// let wide = render_ref(&doc, 2, 120);
///
/// println!("Narrow (40 chars):\n{}", narrow);
/// println!("Wide (120 chars):\n{}", wide);
/// ```
///
/// ## Complex Document Rendering
///
/// ```rust
/// use typeset::{compile, render_ref, text, comp, nest, grp};
///
/// // Build a function definition layout
/// let function_layout = grp(comp(
///     text("function".to_string()),
///     nest(comp(
///         text("calculateArea(width, height)".to_string()),
///         comp(
///             text("{".to_string()),
///             nest(comp(
///                 text("const area = width * height;".to_string()),
///                 text("return area;".to_string()),
///                 true, false
///             )),
///             false, false
///         ),
///         false, false
///     )),
///     true, false
/// ));
///
/// let doc = compile(function_layout);
///
/// // Render with different formatting styles from one compiled document.
/// let compact = render_ref(&doc, 2, 60);
/// let spacious = render_ref(&doc, 4, 100);
///
/// println!("Compact style:\n{}", compact);
/// println!("Spacious style:\n{}", spacious);
/// ```
///
/// ## Batch Rendering with Different Parameters
///
/// ```rust
/// use typeset::{compile, render_ref, text, comp, nest};
///
/// let layout = nest(comp(
///     text("if condition {".to_string()),
///     nest(text("do_something();".to_string())),
///     true, false
/// ));
///
/// let doc = compile(layout);
///
/// // Render for different contexts
/// let configurations = [
///     ("Mobile", 2, 40),
///     ("Desktop", 4, 80),
///     ("Print", 4, 120),
/// ];
///
/// for (name, tab, width) in configurations {
///     let output = render_ref(&doc, tab, width);
///     println!("{} format:\n{}\n", name, output);
/// }
/// ```
///
/// ## Unlimited Width Rendering
///
/// ```rust
/// use typeset::{compile, render, text, comp};
///
/// let layout = comp(
///     text("This is a very long line".to_string()),
///     text("that would normally break".to_string()),
///     true, false
/// );
///
/// let doc = compile(layout);
///
/// // Disable line wrapping with very large width
/// let single_line = render(doc, 4, 10000);
/// // Will keep everything on one line if the layout allows it
/// ```
///
/// # See Also
///
/// - [`compile()`] - Fast compilation of layouts to documents
/// - [`compile_safe()`] - Safe compilation with error handling  
/// - [`Doc`] - The compiled document type that this function renders
/// - [Layout constructors](crate#layout-constructors) - Functions for building layouts
pub fn render(doc: Box<Doc>, tab: usize, width: usize) -> String {
    render_doc(doc, tab, width)
}

/// Renders a compiled document by reference, without consuming it
///
/// This is the borrowing counterpart to [`render()`]. Because rendering only
/// reads the document, this lets you render the same compiled [`Doc`] multiple
/// times — for example at several widths — without cloning or recompiling it.
///
/// # Arguments
///
/// * `doc` - The compiled document to render, borrowed
/// * `tab` - Number of spaces per indentation level
/// * `width` - Maximum line width for line breaking, counted in characters
///
/// # Returns
///
/// A [`String`] containing the formatted output, identical to what
/// [`render()`] would produce for the same document and parameters.
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
///
/// // Render the same document at several widths without cloning it.
/// let narrow = render_ref(&doc, 2, 5);
/// let wide = render_ref(&doc, 2, 80);
/// assert!(narrow.contains('\n'));
/// assert_eq!(wide, "hello world");
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
        assert!(matches!(result, Err(CompilerError::StackOverflow { .. })));
    }
}
