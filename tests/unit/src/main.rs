use std::env;

mod parser;

use typeset::{compile, render};

fn main() {
    let args: Vec<String> = env::args().collect();
    let dsl = &args[1];
    // Optional tab/width let one-off minimization runs shrink the width instead
    // of padding the input with long strings; the tester relies on the defaults.
    let tab = args.get(2).map_or(2, |arg| arg.parse().unwrap());
    let width = args.get(3).map_or(80, |arg| arg.parse().unwrap());
    match parser::parse(dsl.as_str(), &Vec::new()) {
        Err(error) => panic!("{}", error),
        Ok(layout) => {
            let document = compile(layout);
            let result = render(document, tab, width);
            println!("!!!!output!!!!");
            println!("{}", result)
        }
    }
}
