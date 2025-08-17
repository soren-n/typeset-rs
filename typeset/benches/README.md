# Typeset Performance Benchmarks

This directory contains comprehensive benchmarks for the typeset library to validate performance characteristics and identify optimization opportunities.

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark group
cargo bench -- construction
cargo bench -- compilation  
cargo bench -- rendering

# Run with shorter measurement time (for quick feedback)
cargo bench -- --measurement-time 10

# Generate detailed reports
cargo bench -- --output-format html
```

## Benchmark Categories

### 1. Construction Benchmarks
Measures the cost of building layout trees using the combinator functions:
- **Simple layouts** - Basic composition operations
- **Nested layouts** - Deeply nested structures (5, 10, 20 levels)
- **Wide layouts** - Many sibling elements (10, 50, 100 items)
- **JSON-like structures** - Real-world complex layouts

### 2. Compilation Benchmarks  
Measures the `compile()` step that optimizes layout trees:
- Tests the same layout structures as construction
- Validates that compilation time scales reasonably with layout complexity
- Shows the algorithmic complexity of the layout solver

### 3. Rendering Benchmarks
Measures the `render()` step that produces final text:
- **Width independence** - Shows rendering time is independent of target width
- **Multiple widths** - Same document rendered at different widths (20, 40, 80, 120 chars)
- Validates the two-phase design efficiency claims

### 4. End-to-End Benchmarks
Measures complete workflows from layout construction to final text:
- **Simple workflows** - Basic usage patterns
- **Complex workflows** - JSON-like structures of varying sizes
- **Convenience API** - Tests the `format_layout()` one-step function

### 5. Reuse Efficiency Benchmarks
Demonstrates the efficiency of the two-phase compile/render design:
- **Compile once, render multiple** - Optimal usage pattern
- **Compile each time** - Shows the cost of not reusing compiled documents
- Validates performance claims about the two-phase approach

### 6. Combinator Benchmarks
Measures individual layout combinator performance:
- `text`, `comp`, `line`, `nest`, `pack`, `fix`, `grp`, `seq`
- Helps identify which combinators are most/least expensive
- Useful for optimization guidance

## Performance Expectations

Based on the theoretical foundations and implementation:

1. **Linear complexity** - Performance should scale linearly with input size
2. **Width independence** - Rendering time should not depend significantly on target width
3. **Two-phase efficiency** - Compiling once and rendering multiple times should be much faster than compiling each time
4. **Reasonable constants** - While complexity is linear, the constant factors should be reasonable for practical use

## Interpreting Results

### Throughput Metrics
- Higher throughput (ops/sec) is better
- Look for consistent scaling across different input sizes
- Compare relative performance between different approaches

### Latency Metrics  
- Lower latency (time/op) is better
- Check that growth is linear, not quadratic or exponential
- Identify any unexpected performance cliffs

### Memory Usage
While these benchmarks don't directly measure memory, the bump allocation approach should result in:
- Predictable memory usage patterns
- No memory fragmentation issues  
- Efficient cleanup when documents are dropped

## Benchmark Data Analysis

The benchmarks generate detailed reports including:
- **Time series data** - Performance over multiple runs
- **Distribution plots** - Shows performance consistency
- **Regression detection** - Identifies performance changes over time
- **HTML reports** - Visual analysis of results (when using `criterion` with HTML output)

## Using Benchmark Results

1. **Development** - Run benchmarks locally to verify performance impact of changes
2. **CI/CD** - Include in continuous integration to catch performance regressions  
3. **Optimization** - Use results to identify performance bottlenecks and guide optimization efforts
4. **Documentation** - Performance characteristics can be documented based on benchmark results

## Adding New Benchmarks

When adding new benchmarks:

1. **Focus on real usage** - Benchmark patterns that users actually encounter
2. **Multiple scales** - Test small, medium, and large inputs
3. **Edge cases** - Include pathological cases that might cause performance issues
4. **Comparisons** - Compare different approaches to the same problem

Example of adding a new benchmark group:

```rust
fn bench_new_feature(c: &mut Criterion) {
    let mut group = c.benchmark_group("new_feature");
    
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("test_case", size),
            size,
            |b, &size| {
                let data = create_test_data(size);
                b.iter(|| {
                    // Benchmark the operation
                    perform_operation(&data)
                })
            }
        );
    }
    
    group.finish();
}

// Add to criterion_group! macro
criterion_group!(
    benches,
    bench_construction,
    bench_compilation,
    bench_rendering,
    bench_new_feature  // <-- Add here
);
```

## Performance Monitoring

For ongoing performance monitoring:

1. **Baseline establishment** - Run benchmarks on known-good versions to establish baselines
2. **Regression testing** - Compare results against baselines to detect regressions
3. **Performance budgets** - Set acceptable performance thresholds for different operations
4. **Automated alerts** - Set up CI to alert when performance degrades significantly

This benchmark suite provides a comprehensive foundation for understanding and maintaining the performance characteristics of the typeset library.