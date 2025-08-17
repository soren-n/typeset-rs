use typeset::*;

/// Example: Source code formatter for a simple imperative language
/// Demonstrates practical use case with complex layout decisions

#[derive(Debug, Clone)]
enum Statement {
    Assignment {
        var: String,
        expr: Expression,
    },
    If {
        condition: Expression,
        then_block: Vec<Statement>,
        else_block: Option<Vec<Statement>>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
    #[allow(dead_code)]
    Return(Option<Expression>),
}

#[derive(Debug, Clone)]
enum Expression {
    Variable(String),
    Number(i64),
    String(String),
    BinaryOp {
        left: Box<Expression>,
        op: String,
        right: Box<Expression>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
}

/// Format an expression with proper precedence and grouping
fn format_expression(expr: &Expression) -> Box<Layout> {
    match expr {
        Expression::Variable(name) => text(name.clone()),
        Expression::Number(n) => text(n.to_string()),
        Expression::String(s) => text(format!("\"{}\"", s)),

        Expression::BinaryOp { left, op, right } => {
            let left_layout = format_expression(left);
            let op_layout = text(format!(" {} ", op));
            let right_layout = format_expression(right);

            // Group the binary operation so it breaks as a unit
            grp(comp(
                left_layout,
                comp(op_layout, right_layout, false, false),
                false,
                false,
            ))
        }

        Expression::FunctionCall { name, args } => {
            let name_layout = text(name.clone());
            let open_paren = text("(".to_string());
            let close_paren = text(")".to_string());

            if args.is_empty() {
                comp(
                    name_layout,
                    comp(open_paren, close_paren, false, false),
                    false,
                    false,
                )
            } else {
                let args_layout = format_argument_list(args);
                comp(
                    name_layout,
                    comp(
                        open_paren,
                        comp(args_layout, close_paren, false, false),
                        false,
                        false,
                    ),
                    false,
                    false,
                )
            }
        }
    }
}

/// Format function arguments with intelligent breaking
fn format_argument_list(args: &[Expression]) -> Box<Layout> {
    if args.is_empty() {
        return null();
    }

    let formatted_args = args.iter().enumerate().fold(null(), |acc, (i, arg)| {
        let formatted_arg = format_expression(arg);
        if i == 0 {
            formatted_arg
        } else {
            comp(
                acc,
                comp(text(",".to_string()), formatted_arg, true, false),
                false,
                false,
            )
        }
    });

    // Pack arguments for nice alignment
    pack(seq(formatted_args))
}

/// Format a statement with proper indentation and layout
fn format_statement(stmt: &Statement) -> Box<Layout> {
    match stmt {
        Statement::Assignment { var, expr } => {
            let var_layout = text(var.clone());
            let assign_op = text(" = ".to_string());
            let expr_layout = format_expression(expr);
            let semicolon = text(";".to_string());

            comp(
                var_layout,
                comp(
                    assign_op,
                    comp(expr_layout, semicolon, false, false),
                    false,
                    false,
                ),
                false,
                false,
            )
        }

        Statement::FunctionCall { name, args } => {
            let call_expr = Expression::FunctionCall {
                name: name.clone(),
                args: args.clone(),
            };
            comp(
                format_expression(&call_expr),
                text(";".to_string()),
                false,
                false,
            )
        }

        Statement::Return(maybe_expr) => {
            let return_kw = text("return".to_string());
            match maybe_expr {
                None => comp(return_kw, text(";".to_string()), false, false),
                Some(expr) => {
                    let expr_layout = format_expression(expr);
                    comp(
                        return_kw,
                        comp(expr_layout, text(";".to_string()), true, false),
                        false,
                        false,
                    )
                }
            }
        }

        Statement::If {
            condition,
            then_block,
            else_block,
        } => {
            let if_kw = text("if".to_string());
            let open_paren = text(" (".to_string());
            let close_paren = text(") ".to_string());
            let condition_layout = format_expression(condition);

            let condition_part = comp(
                if_kw,
                comp(
                    open_paren,
                    comp(condition_layout, close_paren, false, false),
                    false,
                    false,
                ),
                false,
                false,
            );

            let then_part = format_block(then_block);

            match else_block {
                None => comp(condition_part, then_part, false, false),
                Some(else_stmts) => {
                    let else_kw = text(" else ".to_string());
                    let else_part = format_block(else_stmts);
                    comp(
                        condition_part,
                        comp(
                            then_part,
                            comp(else_kw, else_part, false, false),
                            false,
                            false,
                        ),
                        false,
                        false,
                    )
                }
            }
        }

        Statement::While { condition, body } => {
            let while_kw = text("while".to_string());
            let open_paren = text(" (".to_string());
            let close_paren = text(") ".to_string());
            let condition_layout = format_expression(condition);

            let condition_part = comp(
                while_kw,
                comp(
                    open_paren,
                    comp(condition_layout, close_paren, false, false),
                    false,
                    false,
                ),
                false,
                false,
            );

            let body_part = format_block(body);
            comp(condition_part, body_part, false, false)
        }
    }
}

/// Format a block of statements with proper braces and indentation
fn format_block(statements: &[Statement]) -> Box<Layout> {
    let open_brace = text("{".to_string());
    let close_brace = text("}".to_string());

    if statements.is_empty() {
        comp(open_brace, close_brace, false, false)
    } else {
        let mut formatted_stmts = null();
        for stmt in statements {
            let formatted_stmt = format_statement(stmt);
            formatted_stmts = match formatted_stmts.as_ref() {
                Layout::Null => formatted_stmt,
                _ => line(formatted_stmts, formatted_stmt),
            };
        }

        let indented_stmts = nest(formatted_stmts);

        comp(
            open_brace,
            comp(
                line(null(), indented_stmts),
                line(null(), close_brace),
                false,
                false,
            ),
            false,
            false,
        )
    }
}

fn main() {
    println!("=== Source Code Formatter Example ===\n");

    // Simple assignment
    let assignment = Statement::Assignment {
        var: "x".to_string(),
        expr: Expression::BinaryOp {
            left: Box::new(Expression::Number(10)),
            op: "+".to_string(),
            right: Box::new(Expression::Variable("y".to_string())),
        },
    };

    println!("Simple assignment:");
    println!("{}", render(compile(format_statement(&assignment)), 2, 40));

    // Function call with multiple arguments
    let func_call = Statement::FunctionCall {
        name: "printf".to_string(),
        args: vec![
            Expression::String("Hello, %s! You are %d years old.".to_string()),
            Expression::Variable("name".to_string()),
            Expression::Variable("age".to_string()),
        ],
    };

    println!("\nFunction call (wide):");
    println!("{}", render(compile(format_statement(&func_call)), 2, 80));

    println!("\nFunction call (narrow):");
    println!("{}", render(compile(format_statement(&func_call)), 2, 30));

    // Complex if statement
    let if_stmt = Statement::If {
        condition: Expression::BinaryOp {
            left: Box::new(Expression::Variable("x".to_string())),
            op: ">".to_string(),
            right: Box::new(Expression::Number(0)),
        },
        then_block: vec![
            Statement::Assignment {
                var: "result".to_string(),
                expr: Expression::FunctionCall {
                    name: "calculate".to_string(),
                    args: vec![
                        Expression::Variable("x".to_string()),
                        Expression::Number(42),
                    ],
                },
            },
            Statement::FunctionCall {
                name: "print".to_string(),
                args: vec![Expression::Variable("result".to_string())],
            },
        ],
        else_block: Some(vec![Statement::FunctionCall {
            name: "print".to_string(),
            args: vec![Expression::String("x is not positive".to_string())],
        }]),
    };

    println!("\n=== Complex If Statement ===");

    println!("\nWide format (80 chars):");
    println!("{}", render(compile(format_statement(&if_stmt)), 2, 80));

    println!("\nNarrow format (40 chars):");
    println!("{}", render(compile(format_statement(&if_stmt)), 2, 40));

    // Nested control flow
    let nested_stmt = Statement::While {
        condition: Expression::BinaryOp {
            left: Box::new(Expression::Variable("i".to_string())),
            op: "<".to_string(),
            right: Box::new(Expression::Variable("max_iterations".to_string())),
        },
        body: vec![
            Statement::If {
                condition: Expression::BinaryOp {
                    left: Box::new(Expression::FunctionCall {
                        name: "is_prime".to_string(),
                        args: vec![Expression::Variable("i".to_string())],
                    }),
                    op: "==".to_string(),
                    right: Box::new(Expression::Number(1)),
                },
                then_block: vec![Statement::FunctionCall {
                    name: "add_to_list".to_string(),
                    args: vec![
                        Expression::Variable("primes".to_string()),
                        Expression::Variable("i".to_string()),
                    ],
                }],
                else_block: None,
            },
            Statement::Assignment {
                var: "i".to_string(),
                expr: Expression::BinaryOp {
                    left: Box::new(Expression::Variable("i".to_string())),
                    op: "+".to_string(),
                    right: Box::new(Expression::Number(1)),
                },
            },
        ],
    };

    println!("\n=== Nested Control Flow ===");

    println!("\nWide format (100 chars):");
    println!(
        "{}",
        render(compile(format_statement(&nested_stmt)), 2, 100)
    );

    println!("\nMedium format (60 chars):");
    println!("{}", render(compile(format_statement(&nested_stmt)), 2, 60));

    println!("\nNarrow format (30 chars):");
    println!("{}", render(compile(format_statement(&nested_stmt)), 2, 30));
}
