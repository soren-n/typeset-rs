//! An S-expression pretty printer in the classic Lisp style: the arguments of
//! a call align under its first argument, so a broken call reads
//!
//! ```text
//! (defun factorial (n)
//!        (if (<= n 1) 1 (* n (factorial (- n 1)))))
//! ```
//!
//! `pack` records the column of the first argument and indents the rest to
//! it; `seq` puts every argument on its own line once one breaks; `grp`
//! lets each nested call fit on its own.

use typeset::*;

enum SExpr {
    Atom(&'static str),
    List(Vec<SExpr>),
}

fn layout(expr: &SExpr) -> Layout {
    match expr {
        SExpr::Atom(s) => text(*s),
        SExpr::List(items) => match items.as_slice() {
            [] => text("()"),
            [head] => fix_unpad(text("("), fix_unpad(layout(head), text(")"))),
            [head, args @ ..] => {
                let args = pack(seq(join_with_spaces(args.iter().map(layout))));
                grp(pad(
                    fix_unpad(text("("), layout(head)),
                    fix_unpad(args, text(")")),
                ))
            }
        },
    }
}

fn main() {
    use SExpr::{Atom, List};
    let factorial = List(vec![
        Atom("defun"),
        Atom("factorial"),
        List(vec![Atom("n")]),
        List(vec![
            Atom("if"),
            List(vec![Atom("<="), Atom("n"), Atom("1")]),
            Atom("1"),
            List(vec![
                Atom("*"),
                Atom("n"),
                List(vec![
                    Atom("factorial"),
                    List(vec![Atom("-"), Atom("n"), Atom("1")]),
                ]),
            ]),
        ]),
    ]);

    let doc = layout(&factorial).compile();
    for width in [80, 40, 20] {
        println!("--- width {width}\n{}", doc.render(2, width));
    }
}
