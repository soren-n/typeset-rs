//! Typeset Compiler Module
//!
//! This module contains the complete compiler implementation for the typeset
//! pretty printing library. The compiler transforms Layout ASTs through
//! multiple intermediate representations before producing the final Doc output.
//!
//! ## Architecture
//!
//! The compiler uses a multi-pass architecture:
//! Layout → Edsl → Serial → LinearDoc → FixedDoc → RebuildDoc →
//! DenullDoc → FinalDoc → Doc → String
//!
//! ## Module Organization
//!
//! - `types/` - Type definitions for Layout, intermediate representations, and Doc
//! - `passes/` - Individual compiler passes  
//! - `render/` - Document rendering system
//! - `memory.rs` - Memory management utilities
//! - `error.rs` - Error types and handling
//! - `constructors.rs` - Layout construction functions

pub mod constructors;
pub mod error;
pub mod memory;
pub mod passes;
pub mod pipeline;
pub mod render;
pub mod types;

// Re-export core types
pub use error::CompilerError;
pub use memory::TwoBufferBumpAllocator;
pub use types::{Doc, Layout};

// Legacy implementation removed - now using modular architecture

// Re-export the main compilation functions using new modular pipeline
pub use pipeline::{compile, compile_safe, compile_safe_with_depth, render};

// Re-export constructor functions using new modular types
// pub use constructors::{
//     format_layout,
// };
