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

mod arena;
mod constructors;
mod denull;
mod doc;
pub mod dsl;
mod ir;
mod layout;
mod normalize;
mod render;
mod rescope;
mod resolve_scopes;
mod serialize;

pub use self::doc::Doc;
pub use self::layout::{Break, Layout, Pad};
pub use self::render::render;

pub use self::constructors::{
    blank_line, braces, brackets, comma, comp, fix, fix_pad, fix_unpad, format_layout, grp,
    join_with, join_with_commas, join_with_lines, join_with_spaces, line, nest, newline, null,
    pack, pad, parens, semicolon, seq, space, text, unpad,
};

// The pipeline, pass by pass. Every module below owns the representation it
// produces; this table is the one place the order is stated.
//
// | Pass             | Lowers                    | Does |
// |------------------|---------------------------|------|
// | `serialize`      | `Layout` → `FixedDoc`     | split into lines at hard breaks (and inside broken sequences), coalesce fixed runs, record scope open/close deltas |
// | `resolve_scopes` | `FixedDoc` → `RebuildDoc` | build, solve, and read back the grp/seq scope graph |
// | `denull`         | `RebuildDoc` → `DenullDoc`| drop null/empty terms, strip term wrappers to prop lists |
// | `normalize`      | `DenullDoc` → `DenullDoc` | eliminate trivial grp/seq, right-associate compositions |
// | `rescope`        | `DenullDoc` → `Doc`       | factor shared nest/pack prefixes, build the `Doc` and its extent tables |
//
// Every representation, the input [`Layout`] included, is a flat structure —
// postorder index arenas or plain vectors — so every pass is a loop (or an
// explicit work-stack walk) and the whole pipeline runs in constant native
// stack: no layout is too deep to compile, and depth shows up as O(depth)
// heap instead. Every pass builds its accumulators in flat `Vec`-backed
// arenas it owns and frees on return. The layout's text buffer is borrowed by
// every representation down the pipeline; every intermediate drops as soon as
// the next representation is built, so peak memory is a narrow window around
// the largest pair of adjacent IRs rather than the sum of all of them. The
// output [`Doc`] is a flat arena whose `Clone`/`Drop`/`Debug` are derived and
// non-recursive by construction.

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
    // node arena is dead once serialized.
    let Layout { nodes, text } = layout;
    let fixed = serialize::serialize(&nodes, &text);
    drop(nodes);

    let denull_doc = {
        let scoped_doc = resolve_scopes::resolve_scopes(&fixed);
        denull::denull(&scoped_doc, &fixed.paths)
    };
    drop(fixed);

    rescope::rescope(normalize::normalize(denull_doc))
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

    // End-to-end deep-safety: every stage runs in constant native stack, so
    // layouts far deeper than a recursive implementation could survive
    // compile, render, and drop.
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
