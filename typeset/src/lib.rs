//! # Typeset: A DSL for Pretty Printing
//!
//! Typeset is a powerful embedded domain-specific language (DSL) for defining source code pretty printers.
//! It provides a clean, compositional approach to formatting structured data with automatic line breaking,
//! indentation, and layout optimization.
//!
//! ## Quick Start
//!
//! ```rust
//! use typeset::{compile, render, text, comp, nest, grp, Pad, Break};
//!
//! // Create a simple layout
//! let layout = comp(
//!     text("function"),
//!     nest(comp(
//!         text("name()"),
//!         text("{ body }"),
//!         Pad::Padded, Break::Breakable
//!     )),
//!     Pad::Padded, Break::Breakable
//! );
//!
//! // Compile and render
//! let doc = compile(layout);
//! let output = render(&doc, 2, 40);
//! println!("{}", output);
//! ```
//!
//! ## Core Concepts
//!
//! ### Layout Constructors
//!
//! Typeset provides several fundamental constructors for building layouts:
//!
//! - **[`text()`]** - Text literals that form the visible content
//! - **[`comp()`]** - Compositions that can break into multiple lines
//! - **[`line()`]** - Forced line breaks
//! - **[`nest()`]** - Indentation for nested content
//! - **[`pack()`]** - Alignment to first element position
//! - **[`grp()`]** - Groups that break together
//! - **[`seq()`]** - Sequences where if one breaks, all break
//! - **[`fix()`]** - Fixed content that never breaks
//!
//! ### Compilation Pipeline
//!
//! [`compile()`] lowers a layout through a multi-pass compiler whose
//! intermediate representations are flat postorder arenas; the pass-by-pass
//! description lives in the (internal) `compiler::pipeline` module docs.
//!
//! ## Architecture Overview
//!
//! The typeset library is organized into several key modules:
//!
//! - **Constructors** - Functions for building layout trees ([`text()`], [`comp()`], [`nest()`], etc.)
//! - **Compiler** - Multi-pass compilation pipeline that optimizes layouts
//! - **Types** - Core data structures for layouts and intermediate representations  
//! - **Render** - Final rendering engine that produces formatted strings.
//!   [`render()`] borrows the [`Doc`], so the same document renders repeatedly
//!   (e.g. at several widths) without cloning or recompiling
//!
//! ## Compilation
//!
//! [`compile()`] is infallible: the pipeline is iterative, so no layout is too
//! deep to compile and there is no depth cap. Layout depth shows up as O(depth)
//! heap, freed once compilation returns.
//!
//! ## Performance
//!
//! Typeset is designed for high performance:
//!
//! - Flat, loop-based compilation: every intermediate representation is a
//!   flat arena folded with plain loops, and text is borrowed through every
//!   pass, copied exactly once into the [`Doc`]'s shared text buffer
//! - Constant-time line-breaking decisions: compilation precomputes each
//!   node's flat extent, so rendering decides breaks by arithmetic instead of
//!   re-measuring subtrees — render cost does not grow with the target width
//! - Constant native stack throughout: the passes and renderer never recurse,
//!   and the output [`Doc`] is a flat `Vec`-backed arena, so cloning or
//!   freeing it is non-recursive too — deep layouts never overflow the stack
//!
//! ## Examples
//!
//! ### Basic Usage
//!
//! ```rust
//! use typeset::*;
//!
//! let layout = join_with_spaces(vec![
//!     text("Hello"),
//!     text("world!"),
//! ]);
//!
//! let result = format_layout(layout, 2, 80);
//! assert_eq!(result, "Hello world!");
//! ```
//!
//! ### Complex Formatting
//!
//! ```rust
//! use typeset::*;
//!
//! let json_object = braces(
//!     join_with_commas(vec![
//!         comp(text("\"name\""), text("\"John\""), Pad::Padded, Break::Breakable),
//!         comp(text("\"age\""), text("30"), Pad::Padded, Break::Breakable),
//!     ])
//! );
//!
//! let result = format_layout(json_object, 2, 40);
//! // Output will adapt to width constraints automatically
//! ```
//!
//! ### With Parser (Optional)
//!
//! For more concise syntax, use the optional parser crate:
//!
//! ```ignore
//! use typeset_parser::layout;
//!
//! let my_layout = layout! {
//!     nest ("function" + "name()")
//!     pack ("{ body }")
//! };
//! ```
//!
//! ## Rust Version Compatibility
//!
//! This crate builds on stable Rust (MSRV: 1.89.0).

// Keep the doc-comment cross-references honest: a stale intra-doc link is a
// hard error under `cargo doc`, so broken references cannot silently rot.
#![deny(rustdoc::broken_intra_doc_links)]

mod compiler;

pub use self::compiler::{
    // Composition axes for `comp`
    Break,
    // Core types
    Doc,
    Layout,
    Pad,
    // Core compilation functions
    compile,

    // Rendering
    render,
};

// Re-export constructor functions
pub use self::compiler::constructors::{
    blank_line,

    braces,

    brackets,
    comma,
    comp,

    fix,
    fix_pad,
    fix_unpad,

    // One-step formatting
    format_layout,
    grp,
    // Joining functions
    join_with,
    join_with_commas,
    join_with_lines,

    join_with_spaces,
    line,
    nest,
    newline,
    // Basic constructors
    null,
    pack,
    // Composition shortcuts
    pad,
    // Wrapping functions
    parens,
    semicolon,
    seq,
    // Convenience constructors
    space,
    text,
    unpad,
};

// Tests are now in the dedicated tests/ directory
