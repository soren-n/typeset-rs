//! Exact-output rendering tests.
//!
//! Every expected string here was produced by the OCaml reference
//! implementation (`tests/tester/bin/oracle.ml`) for the same layout, tab and
//! width, so these pin the renderer to the oracle on the constructs the
//! differential fuzzer exercises at random.

use typeset::*;

fn fmt(layout: Layout, tab: usize, width: usize) -> String {
    layout.compile().render(tab, width)
}

#[test]
fn unpadded_and_padded_compositions() {
    assert_eq!(
        fmt(unpad(text("Hello"), text("World")), 2, 80),
        "HelloWorld"
    );
    assert_eq!(fmt(pad(text("Hello"), text("World")), 2, 80), "Hello World");
}

#[test]
fn hard_line_break() {
    assert_eq!(
        fmt(line(text("First"), text("Second")), 2, 80),
        "First\nSecond"
    );
}

#[test]
fn nest_indents_lines_after_a_break() {
    let layout = unpad(text("Prefix:"), nest(line(text("Indented"), text("text"))));
    assert_eq!(fmt(layout, 2, 80), "Prefix:Indented\n  text");
    assert_eq!(
        fmt(nest(pad(text("aaaa"), text("bbbb"))), 2, 5),
        "  aaaa\n  bbbb"
    );
}

#[test]
fn grp_fits_or_breaks_every_composition() {
    let fits = pad(text("Before"), grp(pad(text("grouped"), text("content"))));
    assert_eq!(fmt(fits, 2, 80), "Before grouped content");
    let breaks = pad(
        text("Before"),
        grp(pad(
            pad(text("grouped"), text("content")),
            pad(text("that"), text("breaks")),
        )),
    );
    assert_eq!(fmt(breaks, 2, 12), "Before\ngrouped\ncontent that\nbreaks");
}

#[test]
fn seq_inside_grp_breaks_all_or_nothing() {
    let layout = grp(seq(pad(pad(text("a"), text("b")), text("c"))));
    assert_eq!(fmt(layout, 2, 3), "a\nb\nc");
}

#[test]
fn grp_inside_seq_keeps_its_own_fit_scope() {
    let layout = seq(grp(pad(pad(text("a"), text("b")), text("c"))));
    assert_eq!(fmt(layout, 2, 3), "a b\nc");
}

#[test]
fn seq_cascades_once_one_composition_breaks() {
    let items = join_with_spaces([text("seq1"), text("seq2"), text("seq3"), text("seq4")]);
    let layout = pad(text("Before"), seq(items));
    assert_eq!(fmt(layout, 2, 15), "Before seq1\n \nseq2\n \nseq3\n \nseq4");
}

#[test]
fn seq_containing_a_hard_line_breaks_everywhere() {
    let layout = seq(pad(pad(text("a"), line(text("b"), text("c"))), text("d")));
    assert_eq!(fmt(layout, 2, 80), "a\nb\nc\nd");
}

#[test]
fn pack_aligns_continuation_lines_to_its_first_column() {
    let layout = pad(
        text("Start"),
        pack(unpad(unpad(text("first"), text("second")), text("third"))),
    );
    assert_eq!(fmt(layout, 2, 20), "Start firstsecond\n      third");

    let call = unpad(
        text("f("),
        unpad(
            nest(pack(pad(pad(text("x"), text("y")), text("z")))),
            text(")"),
        ),
    );
    assert_eq!(fmt(call, 2, 6), "f(x y\n  z)");

    let config = pad(
        text("Config:"),
        pack(join_with_commas([
            pad(text("key1"), text("value1")),
            pad(text("key2"), text("value2")),
            pad(text("longer_key"), text("value3")),
        ])),
    );
    assert_eq!(
        fmt(config, 2, 25),
        "Config: key1 value1, key2\n        value2, \n        longer_key value3"
    );
}

#[test]
fn fix_and_fixed_compositions_never_break() {
    let layout = unpad(
        pad(text("breakable"), text("content")),
        fix(pad(text("fixed"), text("content"))),
    );
    assert_eq!(fmt(layout, 2, 10), "breakable\ncontent\nfixed content");
    let ops = unpad(
        fix_pad(text("!"), text("cond")),
        fix_unpad(text("-"), text(">")),
    );
    assert_eq!(fmt(ops, 2, 3), "! cond\n->");
}

#[test]
fn mixed_wrappers() {
    let layout = grp(seq(pad(
        fix(text("FIXED")),
        nest(pack(join_with_commas([
            text("item_a"),
            text("item_b"),
            text("item_c"),
        ]))),
    )));
    assert_eq!(fmt(layout, 3, 30), "FIXED item_a, item_b, item_c");
}

#[test]
fn null_and_empty_text_vanish() {
    assert_eq!(fmt(null(), 2, 80), "");
    assert_eq!(fmt(text(""), 2, 80), "");
    assert_eq!(fmt(pad(text("Before"), null()), 2, 80), "Before");
}

#[test]
fn empty_lines_are_kept() {
    let layout = unpad(text("Section 1"), unpad(blank_line(), text("Section 2")));
    assert_eq!(fmt(layout, 2, 80), "Section 1\n\nSection 2");
    assert_eq!(fmt(line(text("a"), null()), 2, 80), "a\n");
    assert_eq!(fmt(line(null(), text("a")), 2, 80), "\na");
}

#[test]
fn one_document_renders_at_several_widths() {
    let layout = pad(text("This"), pad(text("is"), pad(text("a"), text("test"))));
    let doc = layout.compile();
    assert_eq!(doc.render(2, 100), "This is a test");
    assert_eq!(doc.render(2, 5), "This\nis a\ntest");
}

#[test]
fn format_layout_is_compile_then_render() {
    let layout = pad(text("Hello"), text("world"));
    assert_eq!(
        format_layout(layout.clone(), 2, 80),
        layout.compile().render(2, 80)
    );
}
