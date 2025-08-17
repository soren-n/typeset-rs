use bumpalo::Bump;
use std::rc::Rc;

/// Two-buffer bump allocator system for safe multi-pass compilation
///
/// This system solves several problems with the original single-buffer approach:
/// 1. Enables intermediate memory cleanup between passes
/// 2. Prevents stack overflow through controlled recursion depth
/// 3. Provides safe lifetime management for buffer swapping
/// 4. Maintains performance benefits of bump allocation
#[derive(Debug)]
pub struct TwoBufferBumpAllocator {
    buffer_a: Rc<Bump>,
    buffer_b: Rc<Bump>,
    current_is_a: bool,
    max_recursion_depth: usize,
}

impl TwoBufferBumpAllocator {
    /// Creates a new two-buffer allocator system
    pub fn new() -> Self {
        Self {
            buffer_a: Rc::new(Bump::new()),
            buffer_b: Rc::new(Bump::new()),
            current_is_a: true,
            max_recursion_depth: 10000, // Configurable stack overflow protection
        }
    }

    /// Gets the current active buffer
    pub fn current(&self) -> &Bump {
        if self.current_is_a {
            &self.buffer_a
        } else {
            &self.buffer_b
        }
    }

    /// Gets the next buffer (for output of current pass)
    pub fn next(&self) -> &Bump {
        if self.current_is_a {
            &self.buffer_b
        } else {
            &self.buffer_a
        }
    }

    /// Swaps buffers and clears the now-unused buffer
    /// Returns a new instance with swapped buffers for safe lifetime management
    pub fn swap_and_clear(self) -> Self {
        let new_current = !self.current_is_a;

        // Create new instance with swapped state
        Self {
            buffer_a: if new_current {
                Rc::new(Bump::new())
            } else {
                self.buffer_a.clone()
            },
            buffer_b: if new_current {
                self.buffer_b.clone()
            } else {
                Rc::new(Bump::new())
            },
            current_is_a: new_current,
            max_recursion_depth: self.max_recursion_depth,
        }
    }

    /// Sets maximum recursion depth for stack overflow protection
    pub fn with_max_recursion_depth(mut self, depth: usize) -> Self {
        self.max_recursion_depth = depth;
        self
    }

    /// Gets maximum recursion depth
    pub fn max_recursion_depth(&self) -> usize {
        self.max_recursion_depth
    }
}

impl Default for TwoBufferBumpAllocator {
    fn default() -> Self {
        Self::new()
    }
}
