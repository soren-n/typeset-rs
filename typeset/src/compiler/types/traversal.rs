//! Iterative traversal machinery for the input `Layout` tree.
//!
//! `Layout` is a deep `Box`-recursive tree, so the compiler-generated `Drop` —
//! which recurses down the `Box` chain — would overflow the native stack when a
//! deep value goes out of scope. `Layout` instead frees itself with a
//! heap-allocated worklist. The worklist *driver* lives here as a trait so the
//! per-type dismantling step stays separate from the fixed drain loop.
//!
//! (The output `Doc` used to share this machinery, but it is now a flat
//! `Vec`-backed arena whose `Drop` is derived and non-recursive by construction,
//! so `Layout` is the sole implementor.)

/// A recursive tree freed iteratively via a heap worklist.
///
/// Implementors provide [`dismantle`](DismantleTree::dismantle), which moves a
/// node's children onto the worklist (taking them out of their `Box` and
/// leaving a leaf placeholder), so the child's own `Box` drop terminates in
/// O(1).
///
/// The `Drop` impl is then just `self.drain()`.
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
