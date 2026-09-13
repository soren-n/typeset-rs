//! Asymptotic guards. These are the only wall-clock assertions in the suite:
//! each bound is an order of magnitude above the linear cost, so a regression
//! to quadratic fails loudly without the bound being flaky.

use std::time::{Duration, Instant};
use typeset::{pad, seq, text};

/// Deeply nested grp/seq scopes (`seq(a + seq(b + ...))`) must compile in
/// linear time: the pipeline carries scope open/close deltas rather than each
/// composition's full enclosing scope stack. A quadratic regression takes
/// tens of seconds at this size; linear takes milliseconds.
#[test]
fn nested_scope_compilation_is_linear() {
    let mut layout = text("a");
    for _ in 0..50_000 {
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
