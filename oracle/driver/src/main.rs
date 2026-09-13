//! Renders a layout given in the DSL, for comparison against the OCaml
//! reference implementation (see `oracle/tester`).
//!
//! Usage: `typeset-differential '<layout dsl>' [tab] [width]`

use std::process::exit;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(src) = args.next() else {
        eprintln!("usage: typeset-differential '<layout dsl>' [tab] [width]");
        exit(2);
    };
    let tab = args
        .next()
        .map_or(2, |a| a.parse().expect("tab is an integer"));
    let width = args
        .next()
        .map_or(80, |a| a.parse().expect("width is an integer"));
    match typeset::dsl::parse(&src) {
        Ok(layout) => println!("{}", layout.compile().render(tab, width)),
        Err(error) => {
            eprintln!("parse error: {error}");
            exit(2);
        }
    }
}
