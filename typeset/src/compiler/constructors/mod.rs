//! Layout constructors and convenience functions
//!
//! This module provides a clean API for building layout trees before passing them to
//! the compilation pipeline. These functions are the primary interface for creating
//! pretty-printed output with automatic line breaking and indentation.
//!
//! # Overview
//!
//! The constructor functions create different types of layout nodes:
//!
//! - **Basic constructors** - [`null`], [`text`], [`text_str`]
//! - **Control constructors** - [`fix`], [`grp`], [`seq`], [`nest`], [`pack`]
//! - **Composition constructors** - [`line`], [`comp`] and shortcuts [`pad`], [`unpad`], etc.
//! - **Convenience constructors** - [`space`], [`comma`], [`newline`], etc.
//! - **Joining functions** - [`join_with`], [`join_with_spaces`], etc.
//! - **Wrapping functions** - [`parens`], [`brackets`], [`braces`]
//!
//! # Example Usage
//!
//! ```rust
//! use typeset::*;
//!
//! // Build a function definition layout
//! let layout = comp(
//!     text("function".to_string()),
//!     nest(comp(
//!         text("name()".to_string()),
//!         braces(text("body".to_string())),
//!         true, false
//!     )),
//!     true, false
//! );
//!
//! let output = format_layout(layout, 2, 40);
//! ```

// Re-export all constructor functions
pub mod basic;
pub mod composition;
pub mod control;
pub mod format;
pub mod joining;
pub mod text_utils;
pub mod wrappers;

// Basic constructors
pub use basic::{null, text, text_str};

// Control constructors
pub use control::{fix, grp, nest, pack, seq};

// Composition constructors
pub use composition::{comp, fix_pad, fix_unpad, line, pad, unpad};

// Text utilities
pub use text_utils::{blank_line, comma, newline, semicolon, space};

// Joining functions
pub use joining::{join_with, join_with_commas, join_with_lines, join_with_spaces};

// Wrapper functions
pub use wrappers::{braces, brackets, parens};

// High-level formatting
pub use format::format_layout;
