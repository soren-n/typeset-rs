use std::collections::HashMap;
use typeset::*;

/// Example: JSON pretty printer using typeset layout combinators
/// Demonstrates how to build a practical formatter for a real data structure

#[derive(Debug, Clone)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}

/// Pretty print a JSON value using typeset combinators
fn format_json(value: &JsonValue) -> Box<Layout> {
    match value {
        JsonValue::Null => text("null"),
        JsonValue::Bool(b) => text(b.to_string()),
        JsonValue::Number(n) => text(n.to_string()),
        JsonValue::String(s) => text(format!("\"{}\"", s)),

        JsonValue::Array(arr) => {
            if arr.is_empty() {
                text("[]")
            } else {
                let opening = text("[");
                let closing = text("]");

                // Create comma-separated items
                let items = arr.iter().enumerate().fold(null(), |acc, (i, item)| {
                    let formatted_item = format_json(item);
                    if i == 0 {
                        formatted_item
                    } else {
                        comp(
                            acc,
                            comp(text(","), formatted_item, Pad::Padded, Break::Breakable),
                            Pad::Unpadded,
                            Break::Breakable,
                        )
                    }
                });

                // Group the content - will break all commas if doesn't fit on one line
                let content = grp(seq(items));
                let indented_content = nest(content);

                comp(
                    opening,
                    comp(indented_content, closing, Pad::Unpadded, Break::Breakable),
                    Pad::Unpadded,
                    Break::Breakable,
                )
            }
        }

        JsonValue::Object(obj) => {
            if obj.is_empty() {
                text("{}")
            } else {
                let opening = text("{");
                let closing = text("}");

                // Create comma-separated key-value pairs
                let pairs: Vec<_> = obj.iter().collect();
                let items = pairs
                    .iter()
                    .enumerate()
                    .fold(null(), |acc, (i, (key, value))| {
                        let key_layout = text(format!("\"{}\"", key));
                        let colon = text(": ");
                        let value_layout = format_json(value);

                        let pair = comp(
                            key_layout,
                            comp(colon, value_layout, Pad::Unpadded, Break::Breakable),
                            Pad::Unpadded,
                            Break::Breakable,
                        );

                        if i == 0 {
                            pair
                        } else {
                            comp(
                                acc,
                                comp(text(","), pair, Pad::Padded, Break::Breakable),
                                Pad::Unpadded,
                                Break::Breakable,
                            )
                        }
                    });

                // Group and sequence for proper breaking
                let content = grp(seq(items));
                let indented_content = nest(content);

                comp(
                    opening,
                    comp(indented_content, closing, Pad::Unpadded, Break::Breakable),
                    Pad::Unpadded,
                    Break::Breakable,
                )
            }
        }
    }
}

fn main() {
    println!("=== JSON Pretty Printer Example ===\n");

    // Simple values
    let simple_json = JsonValue::Object({
        let mut map = HashMap::new();
        map.insert(
            "name".to_string(),
            JsonValue::String("John Doe".to_string()),
        );
        map.insert("age".to_string(), JsonValue::Number(30.0));
        map.insert("active".to_string(), JsonValue::Bool(true));
        map.insert("balance".to_string(), JsonValue::Null);
        map
    });

    println!("Simple object (wide):");
    let layout = format_json(&simple_json);
    let doc = compile(layout);
    println!("{}", render(doc.clone(), 2, 80));

    println!("\nSimple object (narrow):");
    println!("{}", render(doc, 2, 20));

    // Complex nested structure
    let complex_json = JsonValue::Object({
        let mut map = HashMap::new();
        map.insert(
            "users".to_string(),
            JsonValue::Array(vec![
                JsonValue::Object({
                    let mut user1 = HashMap::new();
                    user1.insert("id".to_string(), JsonValue::Number(1.0));
                    user1.insert("name".to_string(), JsonValue::String("Alice".to_string()));
                    user1.insert(
                        "roles".to_string(),
                        JsonValue::Array(vec![
                            JsonValue::String("admin".to_string()),
                            JsonValue::String("user".to_string()),
                        ]),
                    );
                    user1
                }),
                JsonValue::Object({
                    let mut user2 = HashMap::new();
                    user2.insert("id".to_string(), JsonValue::Number(2.0));
                    user2.insert("name".to_string(), JsonValue::String("Bob".to_string()));
                    user2.insert(
                        "roles".to_string(),
                        JsonValue::Array(vec![JsonValue::String("user".to_string())]),
                    );
                    user2
                }),
            ]),
        );
        map.insert(
            "metadata".to_string(),
            JsonValue::Object({
                let mut meta = HashMap::new();
                meta.insert("version".to_string(), JsonValue::String("1.0".to_string()));
                meta.insert("timestamp".to_string(), JsonValue::Number(1234567890.0));
                meta
            }),
        );
        map
    });

    println!("\n=== Complex nested structure ===");

    println!("\nWide format (120 chars):");
    let complex_layout = format_json(&complex_json);
    let complex_doc = compile(complex_layout);
    println!("{}", render(complex_doc.clone(), 2, 120));

    println!("\nNarrow format (40 chars):");
    println!("{}", render(complex_doc.clone(), 2, 40));

    println!("\nVery narrow format (20 chars):");
    println!("{}", render(complex_doc, 2, 20));
}
