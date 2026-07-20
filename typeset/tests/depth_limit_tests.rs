//! `compile_safe_with_depth` must actually enforce its depth limit.
//!
//! The limit was previously validated as non-zero and then ignored, so
//! `CompilerError::StackOverflow` could never be returned.

use typeset::*;

fn nested(levels: usize) -> Box<Layout> {
    let mut layout = text("x".to_string());
    for _ in 0..levels {
        layout = nest(layout);
    }
    layout
}

#[test]
fn exceeding_max_depth_is_reported() {
    let result = compile_safe_with_depth(nested(100), 10);
    match result {
        Err(CompilerError::StackOverflow { depth, max_depth }) => {
            assert_eq!(max_depth, 10);
            assert!(depth > 10, "reported depth {depth} should exceed the limit");
        }
        Err(other) => panic!("expected StackOverflow, got {other:?}"),
        Ok(_) => panic!("max_depth was not enforced: 100 levels compiled with a limit of 10"),
    }
}

#[test]
fn layouts_within_max_depth_still_compile() {
    assert!(
        compile_safe_with_depth(nested(10), 1000).is_ok(),
        "a shallow layout was rejected"
    );
}

#[test]
fn zero_max_depth_is_still_rejected() {
    assert!(matches!(
        compile_safe_with_depth(nested(1), 0),
        Err(CompilerError::InvalidInput(_))
    ));
}

#[test]
fn depth_limit_is_not_off_by_one() {
    // A layout of exactly the limit must compile; one deeper must not.
    let depth = _probe_depth();
    assert!(
        compile_safe_with_depth(nested(depth), depth + 1).is_ok(),
        "layout at exactly the limit was rejected"
    );
    assert!(
        compile_safe_with_depth(nested(depth + 5), depth).is_err(),
        "layout past the limit was accepted"
    );
}

/// nest(n) produces n + 1 levels (the wrappers plus the inner text).
fn _probe_depth() -> usize {
    20
}
