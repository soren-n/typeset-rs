use std::fmt;

/// Errors that can occur during compilation.
#[derive(Debug, Clone)]
pub enum CompilerError {
    /// The layout was deeper than the configured `max_depth` and was rejected
    /// before compiling.
    ///
    /// The whole pipeline is iterative, so this is a policy/resource bound
    /// (capping the O(depth) heap an untrusted layout can consume), not an
    /// imminent native-stack overflow. See
    /// [`compile_safe_with_depth`](crate::compile_safe_with_depth).
    DepthLimitExceeded { depth: usize, max_depth: usize },
    /// A caller-supplied argument was invalid (e.g. `max_depth == 0`).
    InvalidInput(String),
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompilerError::DepthLimitExceeded { depth, max_depth } => {
                write!(
                    f,
                    "Layout depth {} exceeded the configured limit of {}",
                    depth, max_depth
                )
            }
            CompilerError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for CompilerError {}
