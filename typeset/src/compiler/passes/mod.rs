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
//! 7. identities - DenullDoc → DenullDoc (remove grp/seq identities)
//! 8. reassociate - DenullDoc → DenullDoc (reassociate compositions)
//! 9. rescope - DenullDoc → FinalDoc (rescope nest and pack)
//! 10. heap - FinalDoc → Doc (move to heap)

pub mod broken;
pub mod denull;
pub mod fixed;
pub mod heap;
pub mod identities;
pub mod linearize;
pub mod reassociate;
pub mod rescope;
pub mod serialize;
pub mod structurize;

// Re-export all pass functions
pub use broken::broken;
pub use denull::denull;
pub use fixed::fixed;
pub use heap::move_to_heap;
pub use identities::identities;
pub use linearize::linearize;
pub use reassociate::reassociate;
pub use rescope::rescope;
pub use serialize::serialize;
pub use structurize::structurize;
