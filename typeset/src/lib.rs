//! # Typeset: A DSL for Pretty Printing
//!
//! Typeset is a powerful embedded domain-specific language (DSL) for defining source code pretty printers.
//! It provides a clean, compositional approach to formatting structured data with automatic line breaking,
//! indentation, and layout optimization.
//!
//! ## Quick Start
//!
//! ```rust
//! use typeset::{compile, render, text, comp, nest, grp};
//!
//! // Create a simple layout
//! let layout = comp(
//!     text("function"),
//!     nest(comp(
//!         text("name()"),
//!         text("{ body }"),
//!         true, false
//!     )),
//!     true, false
//! );
//!
//! // Compile and render
//! let doc = compile(layout);
//! let output = render(doc, 2, 40);
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
//! The library uses a sophisticated multi-pass compiler that transforms layouts
//! through several intermediate representations, each lowered in a fresh bump
//! arena:
//!
//! ```text
//! Layout → Edsl → Serial → LinearDoc → FixedDoc → RebuildDoc →
//! DenullDoc → FinalDoc → Doc → String
//! ```
//!
//! The `DenullDoc → DenullDoc` identity-removal and reassociation passes are
//! elided above for brevity. This pipeline ensures optimal layout decisions and
//! efficient memory usage.
//!
//! ## Architecture Overview
//!
//! The typeset library is organized into several key modules:
//!
//! - **Constructors** - Functions for building layout trees ([`text()`], [`comp()`], [`nest()`], etc.)
//! - **Compiler** - Multi-pass compilation pipeline that optimizes layouts
//! - **Types** - Core data structures for layouts and intermediate representations  
//! - **Render** - Final rendering engine that produces formatted strings. Use
//!   [`render()`] to render a document once, or [`render_ref()`] to render the
//!   same [`Doc`] repeatedly (e.g. at several widths) without cloning it
//!
//! ## Compilation modes
//!
//! - **[`compile()`]** - Infallible. The pipeline is iterative, so no layout is
//!   too deep to compile and there is no depth cap.
//! - **[`compile_within_depth()`]** - Rejects layouts deeper than a
//!   caller-supplied bound with [`DepthLimitExceeded`]. The bound is a resource
//!   limit (it caps the O(depth) heap an untrusted layout can allocate), not a
//!   stack-safety guard.
//!
//! ## Performance
//!
//! Typeset is designed for high performance:
//!
//! - Zero-copy transformations using bump allocation
//! - Optimal line breaking algorithms
//! - Fully iterative pipeline (passes, heap conversion, renderer, and [`Doc`]
//!   drop all run in constant native stack, so deep layouts never overflow it)
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
//!         comp(text("\"name\""), text("\"John\""), true, false),
//!         comp(text("\"age\""), text("30"), true, false),
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
    // Error handling
    DepthLimitExceeded,
    // Core types
    Doc,
    Layout,
    // Core compilation functions
    compile,
    compile_within_depth,

    // Rendering
    render,
    render_ref,
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
