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
//! ### Compile, then render
//!
//! [`compile()`] lowers a [`Layout`] into a [`Doc`]; [`render()`] (or
//! [`Doc::render`]) lays a document out at a tab width and a target line
//! width. Rendering only borrows the document, so one compiled document can
//! be rendered at several widths. Both steps are infallible and run in
//! constant native stack: a layout of any depth compiles and renders, with
//! depth costing heap rather than stack. Break decisions are O(1), so render
//! cost does not grow with the target width.
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
//! ### The DSL
//!
//! The `typeset-parser` crate's `layout!` macro accepts a compact DSL at
//! compile time; [`dsl::parse`] accepts the same language at run time:
//!
//! ```rust
//! let layout = typeset::dsl::parse(r#"nest ("function" + "name()") @ "{ body }""#)?;
//! assert_eq!(layout.compile().render(2, 40), "  function name()\n{ body }");
//! # Ok::<(), typeset::dsl::ParseError>(())
//! ```
//!
//! ## Rust Version Compatibility
//!
//! This crate builds on stable Rust (MSRV: 1.96.0).

// Keep the doc-comment cross-references honest: a stale intra-doc link is a
// hard error under `cargo doc`, so broken references cannot silently rot.
#![deny(rustdoc::broken_intra_doc_links)]

mod compiler;
pub mod dsl;

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
