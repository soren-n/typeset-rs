//! Typeset Compiler Module
//!
//! The compiler transforms Layout ASTs through multiple flat intermediate
//! representations before producing the final Doc output; see [`pipeline`]
//! for the authoritative pass-by-pass description.
//!
//! ## Module Organization
//!
//! - `types/` - The public `Layout` and `Doc`, the arena primitives, and the
//!   IR vocabulary shared between passes
//! - `passes/` - One module per pass; each owns the representation it produces
//! - `render` - Document rendering
//! - `constructors` - Layout construction functions

pub mod constructors;
pub mod passes;
pub mod pipeline;
pub mod render;
pub mod types;

// Re-export core types
pub use types::{Break, Doc, Layout, Pad};

// Re-export the main compilation and rendering functions.
pub use pipeline::compile;
pub use render::render;
