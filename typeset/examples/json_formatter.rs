//! A JSON pretty printer.
//!
//! A container that fits stays on one line; one that does not puts its
//! delimiters and every entry on lines of their own: `seq` makes the
//! delimiters and entries of a broken container break together, `grp` lets
//! each nested container decide for itself, and `nest` indents the entries.
//! A key is fixed to its value, so `"key": {` never splits.

use typeset::*;

enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

fn layout(value: &Json) -> Layout {
    match value {
        Json::Null => text("null"),
        Json::Bool(b) => text(b.to_string()),
        Json::Number(n) => text(n.to_string()),
        Json::String(s) => text(format!("{s:?}")),
        Json::Array(items) if items.is_empty() => text("[]"),
        Json::Array(items) => container("[", "]", items.iter().map(layout)),
        Json::Object(entries) if entries.is_empty() => text("{}"),
        Json::Object(entries) => container(
            "{",
            "}",
            entries.iter().map(|(key, value)| {
                fix_pad(
                    fix_unpad(text(format!("{key:?}")), text(":")),
                    layout(value),
                )
            }),
        ),
    }
}

/// `open`, the comma-separated entries, `close`, all inside one sequence so
/// that a broken container opens and closes on lines of its own.
fn container(open: &str, close: &str, entries: impl IntoIterator<Item = Layout>) -> Layout {
    let entries = nest(join_with_commas(entries));
    grp(seq(unpad(unpad(text(open), entries), text(close))))
}

fn main() {
    let user = |id: f64, name: &str, roles: &[&str]| {
        Json::Object(vec![
            ("id".into(), Json::Number(id)),
            ("name".into(), Json::String(name.into())),
            (
                "roles".into(),
                Json::Array(roles.iter().map(|r| Json::String(r.to_string())).collect()),
            ),
        ])
    };
    let document = Json::Object(vec![
        (
            "users".into(),
            Json::Array(vec![
                user(1.0, "Alice", &["admin", "user"]),
                user(2.0, "Bob", &["user"]),
            ]),
        ),
        (
            "metadata".into(),
            Json::Object(vec![
                ("version".into(), Json::String("1.0".into())),
                ("active".into(), Json::Bool(true)),
                ("parent".into(), Json::Null),
            ]),
        ),
    ]);

    let doc = layout(&document).compile();
    for width in [120, 60, 30] {
        println!("--- width {width}\n{}", doc.render(2, width));
    }
}
