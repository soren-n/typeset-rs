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
//!     text("function".to_string()),
//!     nest(comp(
//!         text("name()".to_string()),
//!         text("{ body }".to_string()),
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
//! The library uses a sophisticated multi-pass compiler that transforms layouts through
//! several intermediate representations:
//!
//! ```text
//! Layout → Edsl → Serial → LinearDoc → FixedDoc → RebuildDoc →
//! DenullDoc → FinalDoc → Doc → String
//! ```
//!
//! This pipeline ensures optimal layout decisions and efficient memory usage.
//!
//! ## Architecture Overview
//!
//! The typeset library is organized into several key modules:
//!
//! - **Constructors** - Functions for building layout trees ([`text()`], [`comp()`], [`nest()`], etc.)
//! - **Compiler** - Multi-pass compilation pipeline that optimizes layouts
//! - **Types** - Core data structures for layouts and intermediate representations  
//! - **Render** - Final rendering engine that produces formatted strings
//! - **Memory** - Efficient bump allocation for zero-copy transformations
//!
//! ## Error Handling
//!
//! The library provides both safe and unsafe compilation modes:
//!
//! - **[`compile()`]** - Fast compilation (may panic on stack overflow)
//! - **[`compile_safe()`]** - Safe compilation with error handling
//! - **[`compile_safe_with_depth()`]** - Safe compilation with custom recursion limits
//!
//! ## Performance
//!
//! Typeset is designed for high performance:
//!
//! - Zero-copy transformations using bump allocation
//! - Optimal line breaking algorithms  
//! - Efficient memory management with controlled recursion
//! - Support for large documents without stack overflow
//!
//! ## Examples
//!
//! ### Basic Usage
//!
//! ```rust
//! use typeset::*;
//!
//! let layout = join_with_spaces(vec![
//!     text("Hello".to_string()),
//!     text("world!".to_string()),
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
//!         comp(text("\"name\"".to_string()), text("\"John\"".to_string()), true, false),
//!         comp(text("\"age\"".to_string()), text("30".to_string()), true, false),
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
//! This crate works on stable Rust (MSRV: 1.89.0). The test suite includes some components
//! that use unstable features like `box_patterns`, but the core library and procedural
//! macro work perfectly on stable Rust.
//!
//! ## Version 2.0 Changes
//!
//! Version 2.0 introduced a major architectural refactoring:
//!
//! - **Modular compiler passes** - Each compilation phase is now separate and testable
//! - **Improved error handling** - Better stack overflow protection and error reporting  
//! - **Enhanced performance** - More efficient memory management and faster compilation
//! - **Better testing** - Comprehensive test suite with integration and performance tests
//!
//! The public API remains unchanged, ensuring seamless migration from v1.x.

mod avl;
mod compiler;
mod list;
mod map;
mod order;
mod util;

pub use self::compiler::{
    // Error handling
    CompilerError,
    // Core types
    Doc,
    Layout,

    TwoBufferBumpAllocator,
    // Core compilation functions
    compile,
    compile_safe,
    compile_safe_with_depth,

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
    text_str,
    unpad,
};

// Tests are now in the dedicated tests/ directory
