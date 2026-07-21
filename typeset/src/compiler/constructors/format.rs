//! High-level one-step formatting.

use crate::compiler::types::Layout;

/// Compiles and renders a layout in one step: `render(compile(layout), tab,
/// width)`.
///
/// `tab` is the number of spaces per indentation level; `width` is the target
/// line width for breaking decisions (not a hard limit — fixed content may
/// exceed it). To format the same layout repeatedly, prefer [`crate::compile`]
/// once with [`crate::render()`] per call. Uses the panicking [`crate::compile`]
/// path; the pipeline is iterative, so it does not overflow the stack on deep
/// input. For fallible compilation use [`crate::compile_safe`].
///
/// ```rust
/// use typeset::*;
/// assert_eq!(format_layout(text("Hello, world!"), 2, 80), "Hello, world!");
/// ```
pub fn format_layout(layout: Box<Layout>, tab: usize, width: usize) -> String {
    use crate::compiler::pipeline;
    let doc = pipeline::compile(layout);
    pipeline::render(doc, tab, width)
}
