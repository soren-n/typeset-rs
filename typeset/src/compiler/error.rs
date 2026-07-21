use std::fmt;

/// Errors that can occur during compilation
#[derive(Debug, Clone)]
pub enum CompilerError {
    /// The layout was deeper than the configured `max_depth` and was rejected
    /// before compiling.
    ///
    /// The name is historical: the pipeline is now fully iterative, so this is
    /// a policy/resource bound (capping the O(depth) heap an untrusted layout
    /// can consume), not an imminent native-stack overflow. Kept as-is for API
    /// stability. See [`compile_safe_with_depth`](crate::compile_safe_with_depth).
    StackOverflow { depth: usize, max_depth: usize },
    /// A caller-supplied argument was invalid (e.g. `max_depth == 0`).
    InvalidInput(String),
    /// Never constructed: bump allocation failure aborts rather than reporting.
    AllocationFailed(String),
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompilerError::StackOverflow { depth, max_depth } => {
                write!(
                    f,
                    "Stack overflow: depth {} exceeded maximum {}",
                    depth, max_depth
                )
            }
            CompilerError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            CompilerError::AllocationFailed(msg) => write!(f, "Allocation failed: {}", msg),
        }
    }
}

impl std::error::Error for CompilerError {}
