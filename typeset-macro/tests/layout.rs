//! The macro expands to the same layouts the run-time parser builds:
//! `Layout`'s `Display` is the DSL, so the two print identically.

use typeset::Layout;
use typeset_macro::layout;

fn same(macro_layout: Layout, src: &str) {
    let expected: Layout = src.parse().expect("parses");
    assert_eq!(macro_layout.to_string(), expected.to_string(), "{src}");
}

#[test]
fn operators_and_wrappers() {
    same(layout! { "a" & "b" }, r#""a" & "b""#);
    same(layout! { "a" + "b" }, r#""a" + "b""#);
    same(layout! { "a" !& "b" + "c" }, r#""a" !& "b" + "c""#);
    same(layout! { "a" !+ "b" }, r#""a" !+ "b""#);
    same(layout! { "a" @ "b" @@ "c" }, r#""a" @ "b" @@ "c""#);
    same(
        layout! { fix ("a" + "b") @ grp (nest ("c" + "d")) @ seq (pack ("e" + "f" + "g")) },
        r#"fix ("a" + "b") @ grp (nest ("c" + "d")) @ seq (pack ("e" + "f" + "g"))"#,
    );
    same(layout! { null + "a" & null }, r#"null + "a" & null"#);
    same(layout! { "esc\"aped\n" }, r#""esc\"aped\n""#);
}

#[test]
fn variables_are_layouts_in_scope() {
    let name = typeset::text("x");
    let items = [typeset::text("p"), typeset::text("q")];
    let first = items[0].clone();
    same(
        layout! { name + (first & "," + name) },
        r#""x" + ("p" & "," + "x")"#,
    );
}

#[test]
fn chains_associate_right() {
    same(layout! { "a" + "b" @ "c" }, r#""a" + ("b" @ "c")"#);
}
