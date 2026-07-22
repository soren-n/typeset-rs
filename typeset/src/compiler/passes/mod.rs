//! Compiler passes for the typeset pretty printing library.
//!
//! Each submodule is one pass. The authoritative pass order and the
//! intermediate representation each pass produces are documented in
//! [`pipeline`](crate::compiler::pipeline).

pub mod denull;
pub mod flatten;
pub mod normalize;
pub mod rescope;
pub mod resolve_breaks;
pub mod resolve_scopes;
pub mod serialize;
pub mod split_lines;

// Re-export all pass functions
pub use denull::denull;
pub use flatten::flatten;
pub use normalize::normalize;
pub use rescope::rescope;
pub use resolve_breaks::resolve_breaks;
pub use resolve_scopes::resolve_scopes;
pub use serialize::serialize;
pub use split_lines::split_lines;
