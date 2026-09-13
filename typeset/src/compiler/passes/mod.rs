//! Compiler passes for the typeset pretty printing library.
//!
//! Each submodule is one pass and owns the representation it produces. The
//! authoritative pass order is documented in
//! [`pipeline`](crate::compiler::pipeline).

pub mod denull;
pub mod normalize;
pub mod rescope;
pub mod resolve_scopes;
pub mod serialize;

pub use denull::denull;
pub use normalize::normalize;
pub use rescope::rescope;
pub use resolve_scopes::resolve_scopes;
pub use serialize::serialize;
