//! Integration tests for the typeset compiler
//!
//! These tests verify that the modular compiler architecture works correctly
//! and produces the same results as the original monolithic implementation.

use typeset::{
    Break, DepthLimitExceeded, Pad, comp, compile, compile_within_depth, grp, line, nest, pack,
    render, seq, text,
};

/// Test basic compilation functionality
#[test]
fn test_basic_compilation() {
    let layout = comp(
        text("Hello"),
        text("World"),
        Pad::Unpadded,
        Break::Breakable,
    );

    // A bounded compile of a shallow layout succeeds...
    let bounded_result = compile_within_depth(layout.clone(), 10000);
    assert!(bounded_result.is_ok());

    let bounded_output = render(bounded_result.unwrap(), 2, 80);

    // ...and produces identical output to the infallible path.
    let original_output = render(compile(layout), 2, 80);
    assert_eq!(
        bounded_output, original_output,
        "Bounded and infallible compile should produce identical output"
    );
    assert_eq!(bounded_output, "HelloWorld");
}

/// Test padded composition
#[test]
fn test_padded_composition() {
    let layout = comp(text("Hello"), text("World"), Pad::Padded, Break::Breakable);

    let doc = compile(layout);
    let output = render(doc, 2, 80);
    assert_eq!(output, "Hello World");
}

/// A depth bound of 0 rejects even the shallowest layout.
#[test]
fn test_depth_bound_rejection() {
    let result = compile_within_depth(text("test"), 0);
    assert_eq!(
        result.unwrap_err(),
        DepthLimitExceeded {
            depth: 1,
            max_depth: 0
        }
    );
}

/// Test complex nested layout
#[test]
fn test_complex_layout() {
    let complex_layout = comp(
        comp(text("fn"), text("main"), Pad::Padded, Break::Breakable),
        comp(
            text("()"),
            comp(
                text("{"),
                comp(
                    text("println!"),
                    text("(\"Hello, World!\");"),
                    Pad::Unpadded,
                    Break::Breakable,
                ),
                Pad::Unpadded,
                Break::Breakable,
            ),
            Pad::Unpadded,
            Break::Breakable,
        ),
        Pad::Padded,
        Break::Breakable,
    );

    let result = compile_within_depth(complex_layout, 10000);
    assert!(result.is_ok());

    if let Ok(doc) = result {
        let output = render(doc, 4, 40);
        assert!(!output.is_empty());
        assert!(output.contains("fn"));
        assert!(output.contains("main"));
        assert!(output.contains("println!"));
    }
}

/// Test line breaks and formatting
#[test]
fn test_line_breaks() {
    let layout = line(text("First line"), text("Second line"));

    let doc = compile(layout);
    let output = render(doc, 2, 80);
    assert_eq!(output, "First line\nSecond line");
}

/// Test nesting and indentation
#[test]
fn test_nesting() {
    let layout = comp(
        text("Prefix:"),
        nest(line(text("Indented"), text("text"))),
        Pad::Unpadded,
        Break::Breakable,
    );

    let doc = compile(layout);
    let output = render(doc, 2, 80);

    assert!(output.contains("Prefix:"));
    assert!(output.contains("Indented"));
    // Check that there is indentation (spaces at the beginning of line)
    assert!(output.contains("  text"));
}

/// Test grouping behavior
#[test]
fn test_grouping() {
    // Test that fits on one line
    let layout = comp(
        text("Before"),
        grp(comp(
            text("grouped"),
            text("content"),
            Pad::Padded,
            Break::Breakable,
        )),
        Pad::Padded,
        Break::Breakable,
    );

    let doc = compile(layout);
    let output = render(doc, 2, 80);
    assert_eq!(output, "Before grouped content");
}

/// Test sequence breaking
#[test]
fn test_sequence_breaking() {
    let layout = seq(comp(
        comp(
            text("item1"),
            text("item2"),
            Pad::Unpadded,
            Break::Breakable,
        ),
        text("item3"),
        Pad::Unpadded,
        Break::Breakable,
    ));

    let doc = compile(layout);
    let output = render(doc, 2, 10); // Narrow width to force breaking

    // All items should be on separate lines due to seq behavior
    let lines: Vec<&str> = output.split('\n').collect();
    assert!(
        lines.len() >= 2,
        "Expected multiple lines due to sequence breaking"
    );
}

/// Test pack alignment
#[test]
fn test_pack_alignment() {
    let layout = comp(
        text("Start"),
        pack(comp(
            comp(
                text("first"),
                text("second"),
                Pad::Unpadded,
                Break::Breakable,
            ),
            text("third"),
            Pad::Unpadded,
            Break::Breakable,
        )),
        Pad::Padded,
        Break::Breakable,
    );

    let doc = compile(layout);
    let output = render(doc, 2, 20); // Narrow width to force alignment

    // Check that alignment works correctly
    assert!(output.contains("Start"));
    if output.contains('\n') {
        let lines: Vec<&str> = output.split('\n').collect();
        // Second and third lines should be aligned
        if lines.len() > 2 {
            let second_indent = lines[1].len() - lines[1].trim_start().len();
            let third_indent = lines[2].len() - lines[2].trim_start().len();
            assert_eq!(
                second_indent, third_indent,
                "Pack alignment should create consistent indentation"
            );
        }
    }
}

/// Test with various rendering widths
#[test]
fn test_different_widths() {
    let layout = comp(
        text("This"),
        comp(
            text("is"),
            comp(text("a"), text("test"), Pad::Padded, Break::Breakable),
            Pad::Padded,
            Break::Breakable,
        ),
        Pad::Padded,
        Break::Breakable,
    );

    // Test wide rendering
    let doc = compile(layout.clone());
    let wide_output = render(doc, 2, 100);
    assert_eq!(wide_output, "This is a test");

    // Test narrow rendering
    let doc = compile(layout);
    let narrow_output = render(doc, 2, 5);

    // Should break into multiple lines when narrow
    assert!(narrow_output.contains('\n') || narrow_output.len() <= 5);
}
