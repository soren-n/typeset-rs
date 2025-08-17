use typeset::*;
use typeset_parser::layout;

/// Example demonstrating the DSL syntax for more concise layout construction.
/// Shows how the macro syntax compares to manual constructor calls.
fn main() {
    println!("=== DSL Syntax Demonstration ===\n");

    // Create some reusable fragments
    let name = text("Alice".to_string());
    let age = text("30".to_string());
    let city = text("New York".to_string());

    // Manual constructor approach
    let manual_layout = comp(
        text("Name:".to_string()),
        comp(
            name.clone(),
            line(
                comp(text("Age:".to_string()), age.clone(), true, false),
                comp(text("City:".to_string()), city.clone(), true, false),
            ),
            true,
            false,
        ),
        true,
        false,
    );

    // DSL approach - much more concise!
    let dsl_layout = layout! {
        "Name:" + name @
        "Age:" + age @
        "City:" + city
    };

    println!("Manual constructor result:");
    println!("{}", render(compile(manual_layout), 2, 40));

    println!("\nDSL syntax result (should be identical):");
    println!("{}", render(compile(dsl_layout), 2, 40));

    println!("\n=== Advanced DSL Features ===");

    // Demonstrate all DSL operators
    let complex_dsl = layout! {
        fix ("fixed" & "together") @
        grp (nest ("indented" + "group")) @@
        seq ("sequential" + "items" + "break" + "together") @
        pack ("packed" + "alignment" + "example")
    };

    println!("\nComplex DSL (wide):");
    println!("{}", render(compile(complex_dsl.clone()), 2, 80));

    println!("\nComplex DSL (narrow):");
    println!("{}", render(compile(complex_dsl), 2, 20));

    // Demonstrate infix fixed operators
    let infix_demo = layout! {
        "start" !& "fixed_to_start" + "normal" + "end" !+ "fixed_to_end"
    };

    println!("\n=== Infix Fixed Operators ===");
    println!("Infix fixed (wide):");
    println!("{}", render(compile(infix_demo.clone()), 2, 80));

    println!("\nInfix fixed (narrow):");
    println!("{}", render(compile(infix_demo), 2, 15));

    // Practical example: function signature formatting
    let function_params = [
        text("param1".to_string()),
        text("param2".to_string()),
        text("very_long_parameter_name".to_string()),
        text("another_param".to_string()),
    ];

    // Build parameter list manually for comparison
    let manual_params = function_params
        .iter()
        .enumerate()
        .fold(null(), |acc, (i, param)| {
            if i == 0 {
                param.clone()
            } else {
                comp(
                    acc,
                    comp(text(",".to_string()), param.clone(), true, false),
                    false,
                    false,
                )
            }
        });

    let manual_function = comp(
        text("function".to_string()),
        comp(
            text("(".to_string()),
            comp(
                pack(seq(manual_params)),
                text(")".to_string()),
                false,
                false,
            ),
            false,
            false,
        ),
        false,
        false,
    );

    // Same thing with DSL
    let param1 = function_params[0].clone();
    let param2 = function_params[1].clone();
    let param3 = function_params[2].clone();
    let param4 = function_params[3].clone();

    let dsl_function = layout! {
        "function" & "(" & pack(seq(param1 & "," + param2 & "," + param3 & "," + param4)) & ")"
    };

    println!("\n=== Function Signature Example ===");

    println!("Manual approach (wide):");
    println!("{}", render(compile(manual_function.clone()), 2, 80));

    println!("\nDSL approach (wide):");
    println!("{}", render(compile(dsl_function.clone()), 2, 80));

    println!("\nManual approach (narrow):");
    println!("{}", render(compile(manual_function), 2, 30));

    println!("\nDSL approach (narrow):");
    println!("{}", render(compile(dsl_function), 2, 30));

    // Document structure example
    let doc_dsl = layout! {
        fix("# ") & "Document Title" @@

        fix("## ") & "Introduction" @
        "This is a sample document demonstrating" +
        "the layout capabilities of the typeset library." @@

        fix("## ") & "Code Example" @
        fix("```rust") @
        nest(
            "fn main() {" @
            nest("println!(\"Hello, World!\");") @
            "}"
        ) @
        fix("```") @@

        fix("## ") & "Conclusion" @
        "The" + "layout" + "system" + "provides" + "flexible" +
        "formatting" + "with" + "intelligent" + "line" + "breaking."
    };

    println!("\n=== Document Formatting Example ===");

    println!("Document (wide - 80 chars):");
    println!("{}", render(compile(doc_dsl.clone()), 2, 80));

    println!("\nDocument (medium - 50 chars):");
    println!("{}", render(compile(doc_dsl.clone()), 2, 50));

    println!("\nDocument (narrow - 30 chars):");
    println!("{}", render(compile(doc_dsl), 2, 30));
}
