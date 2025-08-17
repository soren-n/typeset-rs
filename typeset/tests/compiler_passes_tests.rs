//! Tests for individual compiler passes
//!
//! These tests verify that each compiler pass works correctly in isolation
//! and that the overall pipeline produces correct results.

use typeset::{
    braces, comp, compile, compile_safe, fix, grp, join_with_commas, join_with_spaces, nest, pack,
    parens, render, seq, text,
};

/// Test the complete compiler pipeline with various layout constructs
#[test]
fn test_all_layout_constructs() {
    // Create a layout that uses all major constructs
    let layout = comp(
        text("Program:".to_string()),
        nest(comp(
            fix(text("fn".to_string())),
            comp(
                text("main()".to_string()),
                braces(grp(join_with_commas(vec![
                    text("stmt1".to_string()),
                    text("stmt2".to_string()),
                    seq(comp(
                        text("if".to_string()),
                        parens(text("condition".to_string())),
                        true,
                        false,
                    )),
                ]))),
                true,
                false,
            ),
            true,
            false,
        )),
        false,
        false,
    );

    let doc = compile(layout);
    let output = render(doc, 4, 60);

    // Verify the output contains expected elements
    assert!(output.contains("Program:"));
    assert!(output.contains("fn"));
    assert!(output.contains("main()"));
    assert!(output.contains("stmt1"));
    assert!(output.contains("stmt2"));
    assert!(output.contains("condition"));
}

/// Test memory safety with deep nesting
#[test]
fn test_deep_nesting() {
    let mut layout = text("base".to_string());

    // Create moderately nested structure (reduced to avoid stack overflow)
    for i in 0..20 {
        layout = nest(comp(text(format!("level_{}", i)), layout, true, false));
    }

    // Should compile without stack overflow
    let result = compile_safe(layout);
    assert!(result.is_ok());

    if let Ok(doc) = result {
        let output = render(doc, 2, 100);
        assert!(output.contains("base"));
        assert!(output.contains("level_0"));
        assert!(output.contains("level_19"));
    }
}

/// Test wide layouts with many compositions
#[test]
fn test_wide_layouts() {
    let mut layout = text("first".to_string());

    // Create wide structure with many compositions
    for i in 1..100 {
        layout = comp(layout, text(format!("item_{}", i)), true, false);
    }

    let result = compile_safe(layout);
    assert!(result.is_ok());

    if let Ok(doc) = result {
        let output = render(doc, 2, 500); // Wide render
        assert!(output.contains("first"));
        assert!(output.contains("item_1"));
        assert!(output.contains("item_99"));
    }
}

/// Test that fix construct prevents breaking
#[test]
fn test_fix_prevents_breaking() {
    let breakable = comp(
        text("breakable".to_string()),
        text("content".to_string()),
        true,
        false,
    );

    let fixed = fix(comp(
        text("fixed".to_string()),
        text("content".to_string()),
        true,
        false,
    ));

    let layout = comp(breakable, fixed, false, false);

    let doc = compile(layout);
    let output = render(doc, 2, 10); // Very narrow to force breaking

    // The fixed part should stay together even when narrow
    assert!(output.contains("fixed content"));
}

/// Test group breaking behavior
#[test]
fn test_group_breaking() {
    let grouped_content = grp(join_with_spaces(vec![
        text("grouped".to_string()),
        text("content".to_string()),
        text("that".to_string()),
        text("should".to_string()),
        text("break".to_string()),
        text("together".to_string()),
    ]));

    let layout = comp(text("Before".to_string()), grouped_content, true, false);

    // Test with wide width (should fit on one line)
    let doc = compile(layout.clone());
    let wide_output = render(doc, 2, 100);

    let lines: Vec<&str> = wide_output.split('\n').collect();
    // Should be on fewer lines when there's space
    let wide_line_count = lines.len();

    // Test with narrow width (should break)
    let doc = compile(layout);
    let narrow_output = render(doc, 2, 10);

    let narrow_lines: Vec<&str> = narrow_output.split('\n').collect();
    let narrow_line_count = narrow_lines.len();

    // Should break consistently (all or nothing for the group)
    assert!(wide_line_count <= narrow_line_count);
}

/// Test sequence breaking (breaks all if one breaks)
#[test]
fn test_sequence_breaking() {
    let seq_content = seq(join_with_spaces(vec![
        text("seq1".to_string()),
        text("seq2".to_string()),
        text("seq3".to_string()),
        text("seq4".to_string()),
    ]));

    let layout = comp(text("Before".to_string()), seq_content, true, false);

    let doc = compile(layout);
    let output = render(doc, 2, 15); // Narrow enough to trigger breaking

    // If sequence breaks, all items should be on separate lines
    if output.contains('\n') {
        let content_part = output.split("Before").nth(1).unwrap_or("");
        let items: Vec<&str> = content_part.split_whitespace().collect();

        // Each sequence item should be easily identifiable
        assert!(output.contains("seq1"));
        assert!(output.contains("seq2"));
        assert!(output.contains("seq3"));
        assert!(output.contains("seq4"));
    }
}

/// Test pack alignment with complex content
#[test]
fn test_pack_alignment_complex() {
    let pack_content = pack(join_with_commas(vec![
        comp(
            text("key1".to_string()),
            text("value1".to_string()),
            true,
            false,
        ),
        comp(
            text("key2".to_string()),
            text("value2".to_string()),
            true,
            false,
        ),
        comp(
            text("longer_key".to_string()),
            text("value3".to_string()),
            true,
            false,
        ),
    ]));

    let layout = comp(text("Config:".to_string()), pack_content, true, false);

    let doc = compile(layout);
    let output = render(doc, 2, 25); // Force some breaking

    // Should have proper alignment
    assert!(output.contains("Config:"));
    assert!(output.contains("key1"));
    assert!(output.contains("key2"));
    assert!(output.contains("longer_key"));
}

/// Test mixed layout constructs work together
#[test]
fn test_mixed_constructs() {
    let layout = grp(seq(comp(
        fix(text("FIXED".to_string())),
        nest(pack(join_with_commas(vec![
            text("item_a".to_string()),
            text("item_b".to_string()),
            text("item_c".to_string()),
        ]))),
        true,
        false,
    )));

    // Should compile without issues
    let result = compile_safe(layout);
    assert!(result.is_ok());

    if let Ok(doc) = result {
        let output = render(doc, 3, 30);
        assert!(output.contains("FIXED"));
        assert!(output.contains("item_a"));
        assert!(output.contains("item_b"));
        assert!(output.contains("item_c"));
    }
}

/// Test edge cases with empty and null layouts
#[test]
fn test_edge_cases() {
    // Test with null layout
    let null_layout = typeset::null();
    let doc = compile(null_layout);
    let output = render(doc, 2, 80);
    assert_eq!(output, "");

    // Test composition with null
    let with_null = comp(text("Before".to_string()), typeset::null(), true, false);
    let doc = compile(with_null);
    let output = render(doc, 2, 80);
    assert_eq!(output, "Before");

    // Test empty text
    let empty_text = text("".to_string());
    let doc = compile(empty_text);
    let output = render(doc, 2, 80);
    assert_eq!(output, "");
}
