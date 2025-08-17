use typeset_parser::layout;

fn main() {
    let text_elem = typeset::text("foo".to_string());
    let layout_result = layout! {
      fix (nest (text_elem & "bar")) @
      pack ("baz" !+ text_elem) @@
      grp null + seq (text_elem + text_elem !& text_elem)
    };
    let document = typeset::compile(layout_result.clone());
    println!("---------------------");
    println!("{}", layout_result);
    println!("---------------------");
    println!("{}", document);
    println!("---------------------");
    println!("\"{}\"", typeset::render(document, 2, 80));
    println!("---------------------");
}
