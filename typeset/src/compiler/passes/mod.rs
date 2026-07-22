//! Compiler passes for the typeset pretty printing library
//!
//! This module contains all the individual compiler passes that transform
//! the Layout AST through various intermediate representations before
//! producing the final Doc structure.
//!
//! The passes execute in the following order:
//! 0. flatten - Layout → LayoutArena (flatten the input tree)
//! 1. broken - LayoutArena → EdslDoc (collapse broken sequences)
//! 2. serialize - EdslDoc → Serial (serialize to normalize)
//! 3. fixed - Serial → FixedDoc (lift newlines to spine, coalesce fixed comps)
//! 4. structurize - FixedDoc → RebuildDoc (rebuild with graph structure)
//! 5. denull - RebuildDoc → DenullDoc (remove null identities)
//! 6. normalize - DenullDoc → DenullDoc (eliminate grp/seq identities and
//!    reassociate compositions)
//! 7. rescope - DenullDoc → Doc (rescope nest and pack, into the heap)

pub mod broken;
pub mod denull;
pub mod fixed;
pub mod flatten;
pub mod normalize;
pub mod rescope;
pub mod serialize;
pub mod structurize;

// Re-export all pass functions
pub use broken::broken;
pub use denull::denull;
pub use fixed::fixed;
pub use flatten::flatten;
pub use normalize::normalize;
pub use rescope::rescope;
pub use serialize::serialize;
pub use structurize::structurize;
