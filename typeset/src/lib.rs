#![doc = include_str!("../README.md")]
// Keep the doc-comment cross-references honest: a stale intra-doc link is a
// hard error under `cargo doc`, so broken references cannot silently rot.
#![deny(rustdoc::broken_intra_doc_links)]

mod arena;
mod constructors;
mod doc;
pub mod dsl;
mod emit;
mod graph;
mod layout;
mod lines;
mod render;

pub use self::constructors::{
    comp, fix, fix_pad, fix_unpad, grp, join_with_commas, join_with_lines, join_with_spaces, line,
    nest, null, pack, pad, seq, text, unpad,
};
pub use self::doc::Doc;
pub use self::layout::{Break, Layout, Pad};

// The pipeline: `lines` (Layout -> lines of events) feeding `emit` (lines
// -> Doc, through the scope `graph` of each line) one line at a time, since
// nothing crosses a hard line; each module documents its pass, and
// docs/context/ARCHITECTURE.md the whole. Every representation, the input [`Layout`] included, is a flat
// structure — postorder index arenas or plain vectors — so every pass is a
// loop (or an explicit work-stack walk) and the whole pipeline runs in
// constant native stack: no layout is too deep to compile, and depth shows
// up as O(depth) heap instead. The intermediate is one line, borrowing the
// layout's text. The output [`Doc`] is a flat arena whose
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
        let mut lines = lines::Lines::new(&self.nodes, &self.text);
        let mut emitter = emit::Emitter::new(self.nodes.len());
        while let Some(line) = lines.next_line() {
            emitter.push_line(&line);
        }
        emitter.finish()
    }
}
