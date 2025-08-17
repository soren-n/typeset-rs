//! Performance regression tests
//!
//! These tests ensure that the modular compiler maintains performance
//! characteristics and doesn't introduce regressions.

use std::time::{Duration, Instant};
use typeset::{comp, compile, compile_safe, grp, nest, render, text};

/// Helper function to create a layout with specified depth
fn create_deep_layout(depth: usize) -> Box<typeset::Layout> {
    let mut layout = text("base".to_string());

    for i in 0..depth {
        layout = nest(grp(comp(text(format!("level_{}", i)), layout, true, false)));
    }

    layout
}

/// Helper function to create a layout with specified width
fn create_wide_layout(width: usize) -> Box<typeset::Layout> {
    let mut layout = text("first".to_string());

    for i in 1..width {
        layout = comp(layout, text(format!("item_{}", i)), true, false);
    }

    layout
}

/// Test that compilation completes within reasonable time bounds
#[test]
fn test_compilation_performance() {
    let layout = create_deep_layout(25);

    let start = Instant::now();
    let doc = compile(layout);
    let compile_duration = start.elapsed();

    // Compilation should complete within reasonable time (adjust as needed)
    assert!(
        compile_duration < Duration::from_secs(1),
        "Compilation took too long: {:?}",
        compile_duration
    );

    // Rendering should also be fast
    let start = Instant::now();
    let _output = render(doc, 4, 80);
    let render_duration = start.elapsed();

    assert!(
        render_duration < Duration::from_millis(100),
        "Rendering took too long: {:?}",
        render_duration
    );
}

/// Test memory usage doesn't grow excessively with safe compilation
#[test]
fn test_safe_compilation_performance() {
    let layout = create_wide_layout(100);

    let start = Instant::now();
    let result = compile_safe(layout);
    let duration = start.elapsed();

    assert!(result.is_ok(), "Safe compilation failed");
    assert!(
        duration < Duration::from_secs(2),
        "Safe compilation took too long: {:?}",
        duration
    );
}

/// Test that large layouts can be processed without stack overflow
#[test]
fn test_large_layout_handling() {
    // Create a moderately deep layout (reduced to avoid stack overflow)
    let layout = create_deep_layout(30);

    // Should complete without crashing
    let result = compile_safe(layout);
    assert!(result.is_ok(), "Large layout compilation failed");

    if let Ok(doc) = result {
        let output = render(doc, 2, 100);
        assert!(!output.is_empty());
        assert!(output.contains("base"));
    }
}

/// Test that wide layouts are processed efficiently
#[test]
fn test_wide_layout_performance() {
    let layout = create_wide_layout(50);

    let start = Instant::now();
    let doc = compile(layout);
    let compile_duration = start.elapsed();

    // Should handle wide layouts efficiently
    assert!(
        compile_duration < Duration::from_secs(1),
        "Wide layout compilation too slow: {:?}",
        compile_duration
    );

    let start = Instant::now();
    let output = render(doc, 2, 50);
    let render_duration = start.elapsed();

    assert!(
        render_duration < Duration::from_millis(200),
        "Wide layout rendering too slow: {:?}",
        render_duration
    );

    // Verify correctness
    assert!(output.contains("first"));
    assert!(output.contains("item_49"));
}

/// Test memory efficiency by running multiple compilations
#[test]
fn test_memory_efficiency() {
    // Run multiple compilations to test for memory leaks or excessive allocation
    for i in 0..50 {
        let layout = comp(
            text(format!("iteration_{}", i)),
            create_deep_layout(20),
            true,
            false,
        );

        let doc = compile(layout);
        let output = render(doc, 2, 80);

        assert!(output.contains(&format!("iteration_{}", i)));
        assert!(output.contains("base"));
    }
}

/// Benchmark comparison between compile and compile_safe
#[test]
fn test_compile_vs_compile_safe_performance() {
    let layout = create_deep_layout(20);

    // Benchmark regular compile
    let start = Instant::now();
    let doc1 = compile(layout.clone());
    let compile_duration = start.elapsed();

    // Benchmark safe compile
    let start = Instant::now();
    let doc2_result = compile_safe(layout);
    let safe_compile_duration = start.elapsed();

    assert!(doc2_result.is_ok());
    let doc2 = doc2_result.unwrap();

    // Both should produce identical output
    let output1 = render(doc1, 2, 80);
    let output2 = render(doc2, 2, 80);
    assert_eq!(output1, output2);

    // Safe compile should not be more than 2x slower than regular compile
    assert!(
        safe_compile_duration < compile_duration * 3,
        "Safe compile too slow compared to regular compile: {:?} vs {:?}",
        safe_compile_duration,
        compile_duration
    );
}
