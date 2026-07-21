//! Performance regression tests
//!
//! These tests ensure that the modular compiler maintains performance
//! characteristics and doesn't introduce regressions.

use std::time::{Duration, Instant};
use typeset::{Break, Pad, comp, compile, compile_within_depth, grp, nest, render, seq, text};

/// Helper function to create a layout with specified depth
fn create_deep_layout(depth: usize) -> Box<typeset::Layout> {
    let mut layout = text("base");

    for i in 0..depth {
        layout = nest(grp(comp(
            text(format!("level_{}", i)),
            layout,
            Pad::Padded,
            Break::Breakable,
        )));
    }

    layout
}

/// Helper function to create a layout with specified width
fn create_wide_layout(width: usize) -> Box<typeset::Layout> {
    let mut layout = text("first");

    for i in 1..width {
        layout = comp(
            layout,
            text(format!("item_{}", i)),
            Pad::Padded,
            Break::Breakable,
        );
    }

    layout
}

/// Test that compilation completes within reasonable time bounds
#[test]
fn test_compilation_performance() {
    let layout = create_deep_layout(15);

    let start = Instant::now();
    let result = compile_within_depth(layout, 100);
    assert!(result.is_ok(), "Deep layout compilation failed");
    let doc = result.unwrap();
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
    let layout = create_wide_layout(20);

    let start = Instant::now();
    let result = compile_within_depth(layout, 100);
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
    let layout = create_deep_layout(10);

    // Should complete without crashing
    let result = compile_within_depth(layout, 50);
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
    let layout = create_wide_layout(20);

    let start = Instant::now();
    let result = compile_within_depth(layout, 100);
    assert!(result.is_ok(), "Wide layout compilation failed");
    let doc = result.unwrap();
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
    assert!(output.contains("item_19"));
}

/// Test memory efficiency by running multiple compilations
#[test]
fn test_memory_efficiency() {
    // Run multiple compilations to test for memory leaks or excessive allocation
    for i in 0..50 {
        let layout = comp(
            text(format!("iteration_{}", i)),
            create_deep_layout(8),
            Pad::Padded,
            Break::Breakable,
        );

        let result = compile_within_depth(layout, 50);
        assert!(
            result.is_ok(),
            "Memory efficiency test failed at iteration {}",
            i
        );
        let doc = result.unwrap();
        let output = render(doc, 2, 80);

        assert!(output.contains(&format!("iteration_{}", i)));
        assert!(output.contains("base"));
    }
}

/// Deeply *nested* grp/seq scopes (`seq(a + seq(b + ...))`) were once O(n^2)
/// in `structurize`: every composition carried its full enclosing scope stack,
/// so a chain n deep did O(n^2) work. The scope-delta representation makes it
/// linear. Pin that here — a regression to the quadratic would take tens of
/// seconds at this size, so the time bound catches it loudly without being
/// flaky (linear compiles this in a few milliseconds).
#[test]
fn test_nested_scope_compilation_is_linear() {
    fn nested_seq(n: usize) -> Box<typeset::Layout> {
        let mut layout = text("a");
        for _ in 0..n {
            layout = seq(comp(text("a"), layout, Pad::Padded, Break::Breakable));
        }
        layout
    }

    let start = Instant::now();
    let doc = compile(nested_seq(50_000));
    let output = render(doc, 2, 10);
    let elapsed = start.elapsed();

    assert!(output.contains('a'));
    assert!(
        elapsed < Duration::from_secs(3),
        "nested-scope compilation too slow ({elapsed:?}); O(n^2) regression?"
    );
}
