use typeset::{text, parse, compile, render};

fn main() {
  let foo = text("foo".to_string());
  let layout = parse!("null <&> {0} <//> \"bar\"", foo).unwrap();
  let document = compile(layout.clone());
  let result = render(document.clone(), 2, 80);
  println!("{}", layout);
  println!("-----------");
  println!("{}", document);
  println!("-----------");
  println!("{}", result);
}