//! Line width must be measured in characters, not UTF-8 bytes.
//!
//! `String::len` returns the byte length, so multi-byte text used to be
//! over-measured and broke lines far earlier than the requested width.

use typeset::*;

/// Build `a + b` (padded composition) inside a group and render at `width`.
fn render_pair(a: &str, b: &str, width: usize) -> String {
    let layout = grp(comp(
        text(a.to_string()),
        text(b.to_string()),
        Pad::Padded,
        Break::Breakable,
    ));
    render(&compile(layout), 2, width)
}

#[test]
fn multibyte_text_is_measured_in_characters() {
    // 30 characters, 90 UTF-8 bytes. With " x" the line is 32 columns, which
    // fits in 80; measured as bytes it would be 92 and break.
    let cjk = "日本語".repeat(10);
    assert_eq!(cjk.chars().count(), 30);
    assert_eq!(cjk.len(), 90);

    let out = render_pair(&cjk, "x", 80);
    assert!(
        !out.contains('\n'),
        "multi-byte text broke early; measured as bytes rather than characters:\n{out}"
    );
}

#[test]
fn ascii_and_multibyte_of_equal_length_agree() {
    // Same character count, different byte counts: both must break the same way.
    let ascii = "a".repeat(30);
    let cyrillic = "б".repeat(30); // 2 bytes per character

    // Width 40 discriminates: both are 32 columns with " x" and must fit, but
    // the Cyrillic string is 62 bytes and would break if measured as bytes.
    assert_eq!(
        render_pair(&ascii, "x", 40).contains('\n'),
        render_pair(&cyrillic, "x", 40).contains('\n'),
        "equal-length ascii and multi-byte text broke differently"
    );
}

#[test]
fn multibyte_text_still_breaks_when_genuinely_too_wide() {
    // 30 characters plus " " plus 60 characters exceeds a width of 40.
    let cjk = "日本語".repeat(10);
    let tail = "語".repeat(60);
    let out = render_pair(&cjk, &tail, 40);
    assert!(
        out.contains('\n'),
        "over-wide multi-byte content should still break:\n{out}"
    );
}
