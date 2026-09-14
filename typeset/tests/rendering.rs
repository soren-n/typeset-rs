//! The one deliberate divergence from the reference, which cannot be pinned
//! to it (`oracle.txt` holds everything that can): width is counted in
//! characters, where the reference counts bytes.

use typeset::*;

#[test]
fn width_is_measured_in_characters_not_bytes() {
    // 30 characters, 90 UTF-8 bytes: with " x" the line is 32 columns and
    // fits in 40; measured as bytes it would be 92 and break.
    let cjk = "日本語".repeat(10);
    assert_eq!((cjk.chars().count(), cjk.len()), (30, 90));
    let fits = grp(pad(text(cjk.clone()), text("x")));
    assert!(!fits.compile().render(2, 40).contains('\n'));
    // Over-wide multi-byte content still breaks.
    let wide = grp(pad(text(cjk), text("語".repeat(60))));
    assert!(wide.compile().render(2, 40).contains('\n'));
}
