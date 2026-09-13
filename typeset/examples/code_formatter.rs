//! A source formatter for a small imperative language: blocks with braces on
//! their own lines, call arguments that align under the first argument, and
//! binary operators that break as a unit.

use typeset::*;

enum Expr {
    Var(&'static str),
    Num(i64),
    Str(&'static str),
    Bin(Box<Expr>, &'static str, Box<Expr>),
    Call(&'static str, Vec<Expr>),
}

enum Stmt {
    Assign(&'static str, Expr),
    Call(&'static str, Vec<Expr>),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    While(Expr, Vec<Stmt>),
}

fn expr(e: &Expr) -> Layout {
    match e {
        Expr::Var(v) => text(*v),
        Expr::Num(n) => text(n.to_string()),
        Expr::Str(s) => text(format!("{s:?}")),
        // The operator stays with its left operand; the group breaks the
        // operation as a unit.
        Expr::Bin(l, op, r) => grp(pad(fix_pad(expr(l), text(*op)), expr(r))),
        Expr::Call(name, args) => call(name, args),
    }
}

/// `name(args)`: the arguments align under the first once they break, and
/// each call fits on its own. The opening parenthesis composes breakably so
/// the first argument keeps its `pack`; the closing one is fixed to the last.
fn call(name: &str, args: &[Expr]) -> Layout {
    let args = pack(seq(join_with_commas(args.iter().map(expr))));
    grp(unpad(text(format!("{name}(")), fix_unpad(args, text(")"))))
}

/// `(cond)`, the parentheses fixed to the condition.
fn parens(cond: &Expr) -> Layout {
    fix_unpad(text("("), fix_unpad(expr(cond), text(")")))
}

/// `{`, the statements one per line and indented, `}` on a line of its own.
fn block(stmts: &[Stmt]) -> Layout {
    if stmts.is_empty() {
        return text("{}");
    }
    let body = nest(join_with_lines(stmts.iter().map(stmt)));
    line(line(text("{"), body), text("}"))
}

fn stmt(s: &Stmt) -> Layout {
    match s {
        Stmt::Assign(v, e) => fix_unpad(pad(fix_pad(text(*v), text("=")), expr(e)), text(";")),
        Stmt::Call(name, args) => fix_unpad(call(name, args), text(";")),
        Stmt::If(cond, then, otherwise) => {
            let head = pad(fix_pad(text("if"), parens(cond)), block(then));
            match otherwise {
                None => head,
                Some(stmts) => pad(pad(head, text("else")), block(stmts)),
            }
        }
        Stmt::While(cond, body) => pad(fix_pad(text("while"), parens(cond)), block(body)),
    }
}

fn main() {
    use Expr::{Bin, Call, Num, Str, Var};
    let bin = |l, op, r| Bin(Box::new(l), op, Box::new(r));
    let program = Stmt::While(
        bin(Var("i"), "<", Var("max_iterations")),
        vec![
            Stmt::If(
                bin(Call("is_prime", vec![Var("i")]), "==", Num(1)),
                vec![Stmt::Call("add_to_list", vec![Var("primes"), Var("i")])],
                Some(vec![Stmt::Call(
                    "log",
                    vec![
                        Str("not prime: %d, checked %d so far"),
                        Var("i"),
                        Var("checked"),
                    ],
                )]),
            ),
            Stmt::Assign("i", bin(Var("i"), "+", Num(1))),
        ],
    );

    let doc = stmt(&program).compile();
    for width in [100, 50, 24] {
        println!("--- width {width}\n{}", doc.render(4, width));
    }
}
