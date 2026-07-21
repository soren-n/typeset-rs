use std::fmt;

/// The layout was deeper than the `max_depth` a
/// [`compile_within_depth`](crate::compile_within_depth) call allowed, and was
/// rejected before compiling.
///
/// The whole pipeline is iterative, so this is a policy/resource bound (capping
/// the O(depth) heap an untrusted layout can consume), not an imminent
/// native-stack overflow. Plain [`compile`](crate::compile) has no bound and
/// cannot produce this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthLimitExceeded {
    /// The measured depth of the rejected layout.
    pub depth: usize,
    /// The bound it exceeded.
    pub max_depth: usize,
}

impl fmt::Display for DepthLimitExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Layout depth {} exceeded the configured limit of {}",
            self.depth, self.max_depth
        )
    }
}

impl std::error::Error for DepthLimitExceeded {}
