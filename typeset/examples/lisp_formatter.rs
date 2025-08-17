use typeset::*;

/// Example: Lisp S-expression pretty printer
/// Demonstrates advanced layout techniques like pack() for aligned indentation

#[derive(Debug, Clone)]
enum SExpr {
    Atom(String),
    List(Vec<SExpr>),
}

/// Format an S-expression with proper Lisp-style indentation
/// Uses pack() to align subsequent arguments to the first argument position
fn format_sexpr(expr: &SExpr) -> Box<Layout> {
    match expr {
        SExpr::Atom(s) => text(s.clone()),
        SExpr::List(exprs) => {
            if exprs.is_empty() {
                text("()".to_string())
            } else {
                let open_paren = text("(".to_string());
                let close_paren = text(")".to_string());

                // Format all expressions
                let first = format_sexpr(&exprs[0]);

                if exprs.len() == 1 {
                    // Single item: (item)
                    comp(
                        open_paren,
                        comp(first, close_paren, false, false),
                        false,
                        false,
                    )
                } else {
                    // Multiple items: use pack for alignment
                    let mut rest = null();
                    for expr in &exprs[1..] {
                        let formatted = format_sexpr(expr);
                        rest = match rest.as_ref() {
                            Layout::Null => formatted,
                            _ => comp(rest, formatted, true, false),
                        };
                    }

                    // Pack aligns subsequent lines to the first argument
                    let args = pack(comp(first, rest, true, false));

                    comp(
                        open_paren,
                        comp(args, close_paren, false, false),
                        false,
                        false,
                    )
                }
            }
        }
    }
}

/// Alternative formatter using sequence semantics for different style
fn format_sexpr_sequence(expr: &SExpr) -> Box<Layout> {
    match expr {
        SExpr::Atom(s) => text(s.clone()),
        SExpr::List(exprs) => {
            if exprs.is_empty() {
                text("()".to_string())
            } else {
                let open_paren = text("(".to_string());
                let close_paren = text(")".to_string());

                // Build sequence of all items
                let mut items = null();
                for (i, expr) in exprs.iter().enumerate() {
                    let formatted = format_sexpr_sequence(expr);
                    items = if i == 0 {
                        formatted
                    } else {
                        comp(items, formatted, true, false)
                    };
                }

                // Use sequence so all items break together
                let content = seq(items);
                let indented = nest(content);

                comp(
                    open_paren,
                    comp(indented, close_paren, false, false),
                    false,
                    false,
                )
            }
        }
    }
}

fn main() {
    println!("=== Lisp S-Expression Pretty Printer ===\n");

    // Simple expressions
    let atom = SExpr::Atom("hello".to_string());
    let simple_list = SExpr::List(vec![
        SExpr::Atom("+".to_string()),
        SExpr::Atom("1".to_string()),
        SExpr::Atom("2".to_string()),
        SExpr::Atom("3".to_string()),
    ]);

    println!("Atom: {}", render(compile(format_sexpr(&atom)), 2, 40));
    println!(
        "Simple list (wide): {}",
        render(compile(format_sexpr(&simple_list)), 2, 40)
    );
    println!(
        "Simple list (narrow): {}",
        render(compile(format_sexpr(&simple_list)), 2, 10)
    );

    // Nested expressions
    let nested = SExpr::List(vec![
        SExpr::Atom("defun".to_string()),
        SExpr::Atom("factorial".to_string()),
        SExpr::List(vec![SExpr::Atom("n".to_string())]),
        SExpr::List(vec![
            SExpr::Atom("if".to_string()),
            SExpr::List(vec![
                SExpr::Atom("<=".to_string()),
                SExpr::Atom("n".to_string()),
                SExpr::Atom("1".to_string()),
            ]),
            SExpr::Atom("1".to_string()),
            SExpr::List(vec![
                SExpr::Atom("*".to_string()),
                SExpr::Atom("n".to_string()),
                SExpr::List(vec![
                    SExpr::Atom("factorial".to_string()),
                    SExpr::List(vec![
                        SExpr::Atom("-".to_string()),
                        SExpr::Atom("n".to_string()),
                        SExpr::Atom("1".to_string()),
                    ]),
                ]),
            ]),
        ]),
    ]);

    println!("\n=== Nested Function Definition ===");

    println!("\nPack-aligned style (wide):");
    println!("{}", render(compile(format_sexpr(&nested)), 2, 80));

    println!("\nPack-aligned style (medium):");
    println!("{}", render(compile(format_sexpr(&nested)), 2, 40));

    println!("\nPack-aligned style (narrow):");
    println!("{}", render(compile(format_sexpr(&nested)), 2, 20));

    println!("\n=== Sequence-aligned style (alternative) ===");

    println!("\nSequence style (wide):");
    println!("{}", render(compile(format_sexpr_sequence(&nested)), 2, 80));

    println!("\nSequence style (narrow):");
    println!("{}", render(compile(format_sexpr_sequence(&nested)), 2, 30));

    // Complex data structure
    let complex = SExpr::List(vec![
        SExpr::Atom("let".to_string()),
        SExpr::List(vec![
            SExpr::List(vec![
                SExpr::Atom("x".to_string()),
                SExpr::List(vec![
                    SExpr::Atom("+".to_string()),
                    SExpr::Atom("a".to_string()),
                    SExpr::Atom("very-long-variable-name".to_string()),
                ]),
            ]),
            SExpr::List(vec![
                SExpr::Atom("y".to_string()),
                SExpr::List(vec![
                    SExpr::Atom("*".to_string()),
                    SExpr::Atom("b".to_string()),
                    SExpr::Atom("another-long-name".to_string()),
                ]),
            ]),
        ]),
        SExpr::List(vec![
            SExpr::Atom("format".to_string()),
            SExpr::Atom("t".to_string()),
            SExpr::Atom("\"Result: ~A~%\"".to_string()),
            SExpr::List(vec![
                SExpr::Atom("+".to_string()),
                SExpr::Atom("x".to_string()),
                SExpr::Atom("y".to_string()),
            ]),
        ]),
    ]);

    println!("\n=== Complex Let Expression ===");
    println!("\nComplex (pack style, 50 chars):");
    println!("{}", render(compile(format_sexpr(&complex)), 2, 50));

    println!("\nComplex (sequence style, 50 chars):");
    println!(
        "{}",
        render(compile(format_sexpr_sequence(&complex)), 2, 50)
    );
}
