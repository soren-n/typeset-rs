use typeset::*;

/// Example demonstrating the new convenience API functions
fn main() {
    println!("=== Convenience API Demonstration ===\n");

    // Old way vs new way for common patterns
    println!("=== String Handling ===");

    // text_str convenience
    let old_way = text("Hello".to_string());
    let new_way = text_str("Hello");

    println!("Old: {}", format_layout(old_way, 2, 40));
    println!("New: {}", format_layout(new_way, 2, 40));

    // Joining words with spaces
    println!("\n=== Joining Functions ===");

    let words = vec![
        text_str("The"),
        text_str("quick"),
        text_str("brown"),
        text_str("fox"),
    ];

    // Old way - manual composition
    let manual = pad(
        words[0].clone(),
        pad(words[1].clone(), pad(words[2].clone(), words[3].clone())),
    );

    // New way - join function
    let with_join = join_with_spaces(words);

    println!("Manual composition: {}", format_layout(manual, 2, 40));
    println!("With join function: {}", format_layout(with_join, 2, 40));

    // Comma-separated list
    let items = vec![text_str("apple"), text_str("banana"), text_str("cherry")];
    let comma_list = join_with_commas(items);
    println!("Comma list: {}", format_layout(comma_list, 2, 40));

    // Wrapping functions
    println!("\n=== Wrapping Functions ===");

    let content = text_str("content");

    println!(
        "Parentheses: {}",
        format_layout(parens(content.clone()), 2, 40)
    );
    println!(
        "Brackets: {}",
        format_layout(brackets(content.clone()), 2, 40)
    );
    println!("Braces: {}", format_layout(braces(content), 2, 40));

    // Composition shortcuts
    println!("\n=== Composition Shortcuts ===");

    let left = text_str("left");
    let right = text_str("right");

    println!(
        "Padded: {}",
        format_layout(pad(left.clone(), right.clone()), 2, 40)
    );
    println!(
        "Unpadded: {}",
        format_layout(unpad(left.clone(), right.clone()), 2, 40)
    );
    println!(
        "Fix padded: {}",
        format_layout(fix_pad(left.clone(), right.clone()), 2, 40)
    );
    println!(
        "Fix unpadded: {}",
        format_layout(fix_unpad(left, right), 2, 40)
    );

    // Line joining
    println!("\n=== Line Functions ===");

    let lines = vec![
        text_str("First line"),
        text_str("Second line"),
        text_str("Third line"),
    ];

    let multiline = join_with_lines(lines);
    println!("Joined lines:\n{}", format_layout(multiline, 2, 40));

    // Practical example: function call
    println!("\n=== Practical Example: Function Call ===");

    let function_name = text_str("calculate");
    let args = vec![text_str("x"), text_str("y"), text_str("z")];

    let function_call = unpad(function_name, parens(join_with_commas(args)));

    println!("Function call: {}", format_layout(function_call, 2, 40));

    // Complex example with nesting
    println!("\n=== Complex Example: JSON-like Object ===");

    let key_value =
        |key: &str, value: &str| unpad(unpad(text_str(key), text_str(": ")), text_str(value));

    let object_entries = vec![
        key_value("name", "\"Alice\""),
        key_value("age", "30"),
        key_value("active", "true"),
    ];

    let json_object = braces(nest(join_with_lines(
        vec![null()] // Empty first line
            .into_iter()
            .chain(object_entries.into_iter().enumerate().map(|(i, entry)| {
                if i == 0 {
                    entry
                } else {
                    unpad(comma(), pad(null(), entry))
                }
            }))
            .chain(vec![null()]) // Empty last line
            .collect(),
    )));

    println!(
        "JSON object (wide):\n{}",
        format_layout(json_object.clone(), 2, 80)
    );
    println!(
        "\nJSON object (narrow):\n{}",
        format_layout(json_object, 2, 20)
    );

    // Demonstrate Default trait
    println!("\n=== Default Trait ===");

    let default_layout: Layout = Default::default();
    println!(
        "Default layout: {}",
        format_layout(Box::new(default_layout), 2, 40)
    );
}
