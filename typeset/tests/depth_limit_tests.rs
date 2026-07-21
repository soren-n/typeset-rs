//! `compile_within_depth` must actually enforce its depth limit.
//!
//! The limit was previously validated as non-zero and then ignored, so
//! `DepthLimitExceeded` could never be returned.

use typeset::*;

fn nested(levels: usize) -> Box<Layout> {
    let mut layout = text("x");
    for _ in 0..levels {
        layout = nest(layout);
    }
    layout
}

#[test]
fn exceeding_max_depth_is_reported() {
    match compile_within_depth(nested(100), 10) {
        Err(DepthLimitExceeded { depth, max_depth }) => {
            assert_eq!(max_depth, 10);
            assert!(depth > 10, "reported depth {depth} should exceed the limit");
        }
        Ok(_) => panic!("max_depth was not enforced: 100 levels compiled with a limit of 10"),
    }
}

#[test]
fn layouts_within_max_depth_still_compile() {
    assert!(
        compile_within_depth(nested(10), 1000).is_ok(),
        "a shallow layout was rejected"
    );
}

#[test]
fn zero_max_depth_rejects_everything() {
    // A bound of 0 is not an error; it simply rejects every layout, since the
    // shallowest possible layout has depth 1.
    assert_eq!(compile_within_depth(nested(1), 0).unwrap_err().max_depth, 0);
}

#[test]
fn depth_limit_is_not_off_by_one() {
    // A layout of exactly the limit must compile; one deeper must not.
    let depth = probe_depth();
    assert!(
        compile_within_depth(nested(depth), depth + 1).is_ok(),
        "layout at exactly the limit was rejected"
    );
    assert!(
        compile_within_depth(nested(depth + 5), depth).is_err(),
        "layout past the limit was accepted"
    );
}

/// nest(n) produces n + 1 levels (the wrappers plus the inner text).
fn probe_depth() -> usize {
    20
}
