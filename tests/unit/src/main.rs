#![feature(box_patterns)]

use std::env;

mod parser;

use typeset::{
  compile,
  render
};

fn main() {
  let args: Vec<String> = env::args().collect();
  let dsl = &args[1];
  match parser::parse(dsl.as_str(), &Vec::new()) {
    Err(error) => panic!("{}", error),
    Ok(layout) => {
      let document = compile(layout);
      let result = render(document, 2, 80);
      println!("!!!!output!!!!");
      println!("{}", result)
    }
  }
}