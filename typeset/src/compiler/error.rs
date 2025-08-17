use std::fmt;

/// Errors that can occur during compilation
#[derive(Debug, Clone)]
pub enum CompilerError {
    StackOverflow { depth: usize, max_depth: usize },
    InvalidInput(String),
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
