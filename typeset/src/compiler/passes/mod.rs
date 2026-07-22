//! Compiler passes for the typeset pretty printing library
//!
//! This module contains all the individual compiler passes that transform
//! the Layout AST through various intermediate representations before
//! producing the final Doc structure.
//!
//! The passes execute in the following order:
//! 1. broken - Layout → Edsl (collapse broken sequences)
//! 2. serialize - Edsl → Serial (serialize to normalize)
//! 3. linearize - Serial → LinearDoc (lift newlines to spine)
//! 4. fixed - LinearDoc → FixedDoc (coalesce fixed comps)
//! 5. structurize - FixedDoc → RebuildDoc (rebuild with graph structure)
//! 6. denull - RebuildDoc → DenullDoc (remove null identities)
//! 7. normalize - DenullDoc → DenullDoc (eliminate grp/seq identities and
//!    reassociate compositions)
//! 8. rescope - DenullDoc → Doc (rescope nest and pack, into the heap)

pub mod broken;
pub mod denull;
pub mod fixed;
pub mod linearize;
pub mod normalize;
pub mod rescope;
pub mod serialize;
pub mod structurize;
mod term_chain;
mod walk;

// Re-export all pass functions
pub use broken::broken;
pub use denull::denull;
pub use fixed::fixed;
pub use linearize::linearize;
pub use normalize::normalize;
pub use rescope::rescope;
pub use serialize::serialize;
pub use structurize::structurize;
