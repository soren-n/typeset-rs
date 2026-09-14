#![doc = include_str!("../README.md")]
// Keep the doc-comment cross-references honest: a stale intra-doc link is a
// hard error under `cargo doc`, so broken references cannot silently rot.
#![deny(rustdoc::broken_intra_doc_links)]

mod arena;
mod constructors;
mod doc;
pub mod dsl;
mod layout;
mod render;
mod serialize;
mod structure;

pub use self::constructors::{
    comp, fix, fix_pad, fix_unpad, grp, join_with_commas, join_with_lines, join_with_spaces, line,
    nest, null, pack, pad, seq, text, unpad,
};
pub use self::doc::Doc;
pub use self::layout::{Break, Layout, Pad};

// The pipeline, pass by pass. Every module below owns the representation it
// produces; this table is the one place the order is stated.
//
// | Pass        | Lowers                | Does |
// |-------------|-----------------------|------|
// | `serialize` | `Layout` → `FixedDoc` | split into lines at hard breaks (and inside broken sequences), coalesce runs of fixed compositions, record scope open/close deltas |
// | `structure` | `FixedDoc` → `Doc`    | build and solve the grp/seq scope graph per line, then read it back: drop empty terms, eliminate trivial grp/seq, right-associate, factor shared nest/pack prefixes, build the `Doc` and its extent tables |
//
// Every representation, the input [`Layout`] included, is a flat structure —
// postorder index arenas or plain vectors — so every pass is a loop (or an
// explicit work-stack walk) and the whole pipeline runs in constant native
// stack: no layout is too deep to compile, and depth shows up as O(depth)
// heap instead. Every pass builds its accumulators in flat `Vec`-backed
// arenas it owns and frees on return. The layout's text buffer is borrowed by
// the intermediate representation, and the layout's node arena drops as soon
// as it is serialized. The output [`Doc`] is a flat arena whose
// `Clone`/`Drop`/`Debug` are derived and non-recursive by construction.

impl Layout {
    /// Compiles this layout into a [`Doc`].
    ///
    /// Infallible: the pipeline is iterative, so no layout is too deep to
    /// compile and there is no depth cap. Layout depth shows up as O(depth)
    /// heap, freed once compilation returns.
    ///
    /// ```rust
    /// use typeset::text;
    ///
    /// assert_eq!(text("Hello, world!").compile().render(2, 80), "Hello, world!");
    /// ```
    pub fn compile(self) -> Doc {
        // The layout's text buffer is borrowed all the way down the pipeline;
        // its node arena is dead once serialized.
        let Layout { nodes, text } = self;
        let fixed = serialize::serialize(&nodes, &text);
        drop(nodes);
        structure::structure(&fixed)
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
        let doc = layout.compile();
        let output = doc.render(2, 80);
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
        let doc = layout.compile();
        let output = doc.render(2, 1);
        assert!(output.contains('\n'));
        assert!(output.ends_with('b'));
    }

    #[test]
    fn deep_wrappers_compile_renders_and_drops() {
        // nest/grp/seq wrappers stacked far deeper than a recursive fold
        // could survive; every wrapper but the nests collapses.
        let mut layout = text("x");
        for _ in 0..DEEP {
            layout = nest(grp(seq(layout)));
        }
        let doc = layout.compile();
        assert_eq!(doc.render(1, 80).trim_start(), "x");
    }

    #[test]
    fn deep_fixed_chain_renders() {
        let mut layout = text("z");
        for _ in 0..DEEP {
            layout = comp(text("y"), layout, Pad::Unpadded, Break::Fixed);
        }
        let doc = layout.compile();
        assert_eq!(doc.render(2, 1).len(), DEEP + 1);
    }

    #[test]
    fn render_is_reusable() {
        let layout = comp(text("hello"), text("world"), Pad::Padded, Break::Breakable);
        let doc = layout.compile();
        // Borrowing renders the same document repeatedly without moving it.
        let a = doc.render(2, 5);
        let b = doc.render(2, 80);
        assert_eq!(b, "hello world");
        assert!(a.contains('\n'));
    }
}
