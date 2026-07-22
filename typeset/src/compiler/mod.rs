//! Typeset Compiler Module
//!
//! This module contains the complete compiler implementation for the typeset
//! pretty printing library. The compiler transforms Layout ASTs through
//! multiple intermediate representations before producing the final Doc output.
//!
//! ## Architecture
//!
//! The compiler uses a multi-pass architecture; see [`pipeline`] for the
//! authoritative pass-by-pass description of the intermediate representations.
//!
//! ## Module Organization
//!
//! - `types/` - Type definitions for Layout, intermediate representations, and Doc
//! - `passes/` - Individual compiler passes
//! - `render/` - Document rendering system
//! - `constructors/` - Layout construction functions

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
