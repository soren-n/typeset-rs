//! Shared iterative traversal machinery for the recursive `Box`-tree types.
//!
//! `Layout`, `Doc`, `DocObj`, and `DocObjFix` are all deep recursive trees, so
//! the compiler-generated `Drop` — which recurses down the `Box` chain — would
//! overflow the native stack when a deep value goes out of scope. Each type
//! instead frees itself with a heap-allocated worklist. The worklist *driver*
//! is identical across all of them and lives here once; only the per-type step
//! that moves a node's children onto the worklist differs.

/// A recursive tree freed iteratively via a heap worklist.
///
/// Implementors provide [`dismantle`](DismantleTree::dismantle), which moves a
/// node's same-typed children onto the worklist (taking them out of their
/// `Box` and leaving a leaf placeholder), so the child's own `Box` drop
/// terminates in O(1). Cross-typed children (e.g. a `DocObj` inside a `Doc`)
/// are left in place and freed by their own type's iterative `Drop`.
///
/// The `Drop` impl of each type is then just `self.drain()`.
pub(crate) trait DismantleTree: Sized {
    /// Move this node's same-typed children onto `stack`, leaving leaves in
    /// their place.
    fn dismantle(&mut self, stack: &mut Vec<Self>);

    /// Free the whole tree iteratively: dismantle the root, then repeatedly
    /// dismantle whatever it pushed until the worklist drains. Every value that
    /// actually drops has already had its same-typed children moved out, so no
    /// drop recurses.
    fn drain(&mut self) {
        let mut stack: Vec<Self> = Vec::new();
        self.dismantle(&mut stack);
        while let Some(mut node) = stack.pop() {
            node.dismantle(&mut stack);
        }
    }
}
