use typeset_parser::layout;

fn main() {
    let name = typeset::text("foo");
    let layout = layout! {
      fix (nest (name & "bar")) @
      pack ("baz" !+ name) @@
      grp null + seq (name + name !& name)
    };
    println!("{}", layout.compile().render(2, 80));
}
