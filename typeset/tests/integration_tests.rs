//! Integration tests for the typeset compiler
//!
//! These tests verify that the modular compiler architecture works correctly
//! and produces the same results as the original monolithic implementation.

use typeset::{
    comp, compile, compile_safe, compile_safe_with_depth, grp, line, nest, pack, render, seq, text,
    CompilerError,
};

/// Test basic compilation functionality
#[test]
fn test_basic_compilation() {
    let layout = comp(
        text("Hello".to_string()),
        text("World".to_string()),
        false,
        false,
    );

    // Test the safe compile function
    let safe_result = compile_safe(layout.clone());
    assert!(safe_result.is_ok());

    if let Ok(safe_doc) = safe_result {
        let safe_output = render(safe_doc, 2, 80);

        // Compare with original compile function
        let original_doc = compile(layout);
        let original_output = render(original_doc, 2, 80);

        assert_eq!(
            safe_output, original_output,
            "Safe and original compile should produce identical output"
        );
        assert_eq!(safe_output, "HelloWorld");
    }
}

/// Test padded composition
#[test]
fn test_padded_composition() {
    let layout = comp(
        text("Hello".to_string()),
        text("World".to_string()),
        true,
        false,
    );

    let doc = compile(layout);
    let output = render(doc, 2, 80);
    assert_eq!(output, "Hello World");
}

/// Test error handling with invalid parameters
#[test]
fn test_error_handling() {
    let result = compile_safe_with_depth(text("test".to_string()), 0);
    assert!(result.is_err());

    if let Err(e) = result {
        match e {
            CompilerError::InvalidInput(_) => {
                // Expected error type
            }
            _ => panic!("Expected InvalidInput error, got: {:?}", e),
        }
    }
}

/// Test complex nested layout
#[test]
fn test_complex_layout() {
    let complex_layout = comp(
        comp(
            text("fn".to_string()),
            text("main".to_string()),
            true,
            false,
        ),
        comp(
            text("()".to_string()),
            comp(
                text("{".to_string()),
                comp(
                    text("println!".to_string()),
                    text("(\"Hello, World!\");".to_string()),
                    false,
                    false,
                ),
                false,
                false,
            ),
            false,
            false,
        ),
        true,
        false,
    );

    let result = compile_safe(complex_layout);
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
    let layout = line(
        text("First line".to_string()),
        text("Second line".to_string()),
    );

    let doc = compile(layout);
    let output = render(doc, 2, 80);
    assert_eq!(output, "First line\nSecond line");
}

/// Test nesting and indentation
#[test]
fn test_nesting() {
    let layout = comp(
        text("Prefix:".to_string()),
        nest(line(text("Indented".to_string()), text("text".to_string()))),
        false,
        false,
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
        text("Before".to_string()),
        grp(comp(
            text("grouped".to_string()),
            text("content".to_string()),
            true,
            false,
        )),
        true,
        false,
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
            text("item1".to_string()),
            text("item2".to_string()),
            false,
            false,
        ),
        text("item3".to_string()),
        false,
        false,
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
        text("Start".to_string()),
        pack(comp(
            comp(
                text("first".to_string()),
                text("second".to_string()),
                false,
                false,
            ),
            text("third".to_string()),
            false,
            false,
        )),
        true,
        false,
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
        text("This".to_string()),
        comp(
            text("is".to_string()),
            comp(text("a".to_string()), text("test".to_string()), true, false),
            true,
            false,
        ),
        true,
        false,
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
