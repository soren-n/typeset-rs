# Two-Buffer Bump Allocator System Design

## Overview

This document describes the design and implementation of a two-buffer bump allocator system for the typeset-rs pretty printer compiler. The system was designed to solve several critical issues with the original single-buffer approach while maintaining performance and safety.

## Problems Addressed

### 1. Memory Accumulation
The original `compile()` function uses a single Bump allocator for all 10 sequential passes:
- Layout → Edsl → Serial → LinearDoc → FixedDoc → RebuildDoc → DenullDoc → DenullDoc → DenullDoc → FinalDoc → Doc
- Memory usage grows progressively through all passes
- No intermediate cleanup until the final `_move_to_heap()` call
- Peak memory usage can be 10x larger than necessary

### 2. Stack Overflow Risk
- Deep recursion in transformations without bounds checking
- No configurable limits on recursion depth
- Difficult to debug stack overflow issues in complex layouts

### 3. Lifetime Management Complexity
- All intermediate types tied to single allocator lifetime
- Makes buffer swapping complex due to Rust's borrow checker
- Prevents optimization of memory usage patterns

### 4. Error Handling
- Original function panics on errors rather than returning Results
- No way to recover from compilation failures
- Limited debugging information for issues

## Solution Architecture

### Core Components

#### 1. TwoBufferBumpAllocator
```rust
pub struct TwoBufferBumpAllocator {
    buffer_a: Rc<Bump>,
    buffer_b: Rc<Bump>,
    current_is_a: bool,
    max_recursion_depth: usize,
}
```

**Key Features:**
- Alternating buffer system (A → B → A → ...)
- Reference-counted buffers for safe sharing
- Configurable recursion depth limits
- Safe buffer swapping with automatic cleanup

**Methods:**
- `current()` - Get active buffer for reading
- `next()` - Get destination buffer for writing
- `swap_and_clear()` - Swap buffers and clear the now-unused one

#### 2. CompilerError
```rust
pub enum CompilerError {
    StackOverflow { depth: usize, max_depth: usize },
    InvalidInput(String),
    AllocationFailed(String),
}
```

**Benefits:**
- Better error reporting than panics
- Structured error types for different failure modes
- Recoverable errors for applications

#### 3. Safe Compilation Functions
```rust
pub fn compile_safe(layout: Box<Layout>) -> Result<Box<Doc>, CompilerError>
pub fn compile_safe_with_depth(layout: Box<Layout>, max_depth: usize) -> Result<Box<Doc>, CompilerError>
```

**Features:**
- Result-based error handling
- Configurable stack overflow protection
- Identical output to original `compile()` function
- Future-ready for true two-buffer optimization

## Implementation Challenges & Solutions

### Challenge 1: Rust Lifetime Constraints
**Problem:** Rust's borrow checker prevents holding references to allocators while swapping them.

**Solution:** Currently implemented with individual allocators per pass as a stepping stone:
```rust
// Pass isolation approach (current implementation)
let layout1 = {
    let mem = Bump::new();
    _broken(&mem, layout)
};
// mem dropped here, preventing accumulation
```

**Future Solution:** When lifetime constraints are resolved, implement true buffer alternation:
```rust
// Target implementation (requires advanced lifetime management)
let (result, new_buffers) = pass_with_swap(old_buffers, input, transformation);
```

### Challenge 2: Memory Reuse vs Safety
**Problem:** True buffer reuse requires complex lifetime management.

**Solution:** Hybrid approach:
1. **Current:** Memory isolation (prevents accumulation)
2. **Future:** True two-buffer reuse (maximum efficiency)

### Challenge 3: Maintaining Compatibility  
**Problem:** New system must produce identical results to original.

**Solution:** 
- Extensive test suite comparing outputs
- Same underlying transformation functions
- Wrapper approach preserves all semantics

## Performance Analysis

### Current Implementation Benefits
1. **Memory Isolation:** Prevents progressive accumulation
2. **Early Cleanup:** Each pass cleans up immediately
3. **Predictable Usage:** Bounded memory per pass
4. **Error Recovery:** Graceful failure handling

### Benchmark Results
The benchmark suite (`two_buffer_comparison.rs`) tests:
- Deep layouts (recursion stress)
- Wide layouts (breadth stress) 
- Memory stress tests (large structures)

**Key Findings:**
- Identical output to original compile()
- Similar performance characteristics
- Better memory behavior in stress tests
- Improved error reporting

## Usage Examples

### Basic Usage
```rust
use typeset::{text, comp, compile_safe, render};

let layout = comp(
    text("Hello".to_string()),
    text("World".to_string()),
    false, false
);

match compile_safe(layout) {
    Ok(doc) => {
        let output = render(doc, 2, 80);
        println!("{}", output);
    }
    Err(e) => {
        eprintln!("Compilation failed: {}", e);
    }
}
```

### Advanced Usage with Depth Control
```rust
use typeset::{compile_safe_with_depth, CompilerError};

match compile_safe_with_depth(complex_layout, 5000) {
    Ok(doc) => process_doc(doc),
    Err(CompilerError::StackOverflow { depth, max_depth }) => {
        eprintln!("Stack overflow at depth {} (max: {})", depth, max_depth);
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

## Future Optimizations

### 1. True Buffer Alternation
When Rust lifetime management allows, implement:
- Buffer A → Buffer B transformations
- Immediate cleanup of previous buffer
- 50% memory usage reduction

### 2. Adaptive Allocation
- Dynamic buffer sizing based on input complexity
- Specialized allocators for different pass types
- Memory pooling for repeated compilations

### 3. Parallel Pass Execution
- Independent passes could run in parallel
- Pipeline architecture for streaming compilation
- Multi-threaded buffer management

### 4. Stack Overflow Prevention
- Iterative transformation algorithms
- Trampoline-style execution
- Dynamic recursion depth monitoring

## Testing Strategy

### 1. Functional Tests (`test_two_buffer.rs`)
- Basic functionality verification
- Error handling validation
- Complex layout compilation
- Output comparison with original

### 2. Performance Tests (`two_buffer_comparison.rs`)
- Memory usage profiling
- Speed comparison benchmarks
- Stress testing with large inputs
- Regression detection

### 3. Integration Tests
- End-to-end compilation pipelines
- Real-world layout scenarios  
- Edge case handling
- Error recovery flows

## Migration Guide

### For Library Users
1. **Non-breaking:** Original `compile()` still available
2. **Recommended:** Use `compile_safe()` for better error handling
3. **Advanced:** Use `compile_safe_with_depth()` for stack protection

### For Contributors
1. **Understanding:** Study the pass pipeline architecture
2. **Extending:** Add new passes through the established pattern
3. **Optimizing:** Focus on individual pass efficiency
4. **Testing:** Add cases to the comprehensive test suite

## Conclusion

The two-buffer bump allocator system provides a foundation for memory-efficient and safe pretty-printing compilation. While the current implementation uses a hybrid approach due to Rust lifetime constraints, it successfully addresses the core problems:

✅ **Memory Management:** Prevents accumulation through isolation  
✅ **Error Handling:** Graceful failure with detailed reporting  
✅ **Stack Safety:** Configurable depth limits (framework in place)  
✅ **Compatibility:** Identical output to original system  
✅ **Performance:** Maintains speed while improving memory behavior  
✅ **Extensibility:** Clear architecture for future optimizations  

The system demonstrates that practical solutions can be implemented even when theoretical ideals face implementation constraints, providing immediate benefits while preparing for future enhancements.