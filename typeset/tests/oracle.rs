//! The rendering cases pinned to the OCaml reference: `oracle.txt` holds
//! each layout in the DSL with its tab and width, and the rendering the
//! reference produced for it. The expected blocks are written by
//! `oracle/pin.sh`, never by hand, so this is byte-identity with the
//! reference on hand-picked constructs; the oracle harness covers generated
//! ones.

use typeset::dsl;

const CASES: &str = include_str!("oracle.txt");

#[test]
fn every_pinned_case_renders_as_the_reference_did() {
    let mut cases = 0;
    let mut failures = Vec::new();
    let mut lines = CASES.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(spec) = line.strip_prefix('[') else {
            continue;
        };
        let (dims, layout) = spec
            .split_once("] ")
            .expect("a case header is `[tab width] layout`");
        let (tab, width) = dims.split_once(' ').expect("a tab and a width");
        let (tab, width): (usize, usize) =
            (tab.parse().expect("a tab"), width.parse().expect("a width"));
        let mut expected = Vec::new();
        while let Some(out) = lines.peek().and_then(|l| l.strip_prefix('|')) {
            expected.push(out);
            lines.next();
        }
        let expected = expected.join("\n");
        let actual = dsl::parse(layout)
            .unwrap_or_else(|e| panic!("{layout}: {e}"))
            .compile()
            .render(tab, width);
        cases += 1;
        if actual != expected {
            failures.push(format!(
                "{line}\n--- reference\n{expected}\n--- typeset\n{actual}"
            ));
        }
    }
    assert!(cases > 0, "no cases in oracle.txt");
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n\n"));
}
