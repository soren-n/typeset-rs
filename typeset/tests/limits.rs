//! No depth cap and no worse than linear: every stage runs in constant
//! native stack, so layouts far deeper than a recursive implementation could
//! survive compile, render and drop; and deeply nested scopes compile in
//! linear time. The linearity check is the only wall-clock assertion in the
//! suite: its bound is an order of magnitude above the linear cost, so a
//! regression to quadratic fails loudly without the bound being flaky.

use std::time::{Duration, Instant};
use typeset::*;

const DEEP: usize = 50_000;

#[test]
fn deep_nest_compiles_renders_and_drops() {
    let mut layout = text("x");
    for _ in 0..DEEP {
        layout = nest(layout);
    }
    let output = layout.compile().render(2, 80);
    // Pure nesting introduces no line breaks; only leading indentation.
    assert!(!output.contains('\n'));
    assert!(output.ends_with('x'));
}

#[test]
fn deep_comp_compiles_renders_and_drops() {
    // Left-nested compositions; a narrow width forces a break at each,
    // exercising the renderer's deep break path and the deep `Doc` spine.
    let mut layout = text("a");
    for _ in 0..DEEP {
        layout = comp(layout, text("b"), Pad::Padded, Break::Breakable);
    }
    let output = layout.compile().render(2, 1);
    assert!(output.contains('\n'));
    assert!(output.ends_with('b'));
}

#[test]
fn deep_wrappers_compile_renders_and_drops() {
    // nest/grp/seq wrappers stacked far deeper than a recursive fold could
    // survive; every wrapper but the nests collapses.
    let mut layout = text("x");
    for _ in 0..DEEP {
        layout = nest(grp(seq(layout)));
    }
    assert_eq!(layout.compile().render(1, 80).trim_start(), "x");
}

#[test]
fn deep_fixed_chain_renders() {
    let mut layout = text("z");
    for _ in 0..DEEP {
        layout = comp(text("y"), layout, Pad::Unpadded, Break::Fixed);
    }
    assert_eq!(layout.compile().render(2, 1).len(), DEEP + 1);
}

#[test]
fn deep_parentheses_parse_without_recursion() {
    let depth = 100_000;
    let src = format!("{}\"x\"{}", "(".repeat(depth), ")".repeat(depth));
    let layout = dsl::parse(&src).expect("parses");
    assert_eq!(layout.compile().render(2, 80), "x");
}

/// Deeply nested grp/seq scopes (`seq(a + seq(b + ...))`) must compile in
/// linear time: the pipeline carries scope open/close deltas rather than each
/// composition's full enclosing scope stack. A quadratic regression takes
/// tens of seconds at this size; linear takes milliseconds.
#[test]
fn nested_scope_compilation_is_linear() {
    let mut layout = text("a");
    for _ in 0..DEEP {
        layout = seq(pad(text("a"), layout));
    }
    let start = Instant::now();
    let output = layout.compile().render(2, 10);
    let elapsed = start.elapsed();
    assert!(output.starts_with('a'));
    assert!(
        elapsed < Duration::from_secs(3),
        "nested-scope compilation too slow ({elapsed:?}); O(n^2) regression?"
    );
}
