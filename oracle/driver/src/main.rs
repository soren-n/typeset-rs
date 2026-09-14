//! Renders layouts given in the DSL, for the OCaml oracle harness (see
//! `oracle/tester`), which keeps one driver process for a whole run.
//!
//! One request per line on stdin, `<tab> <width> <layout dsl>`, each
//! answered on stdout with `ok <bytes>` or `error <bytes>` on a line of its
//! own followed by exactly that many bytes: the rendering (which may span
//! lines and has no trailing newline) or the parse error.

use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.expect("stdin is readable");
        let (status, body) = match render(&line) {
            Ok(rendering) => ("ok", rendering),
            Err(message) => ("error", message),
        };
        write!(out, "{status} {}\n{body}", body.len()).expect("stdout is writable");
        out.flush().expect("stdout is writable");
    }
}

fn render(request: &str) -> Result<String, String> {
    let mut fields = request.splitn(3, ' ');
    let mut number = |what: &str| -> Result<usize, String> {
        fields
            .next()
            .and_then(|field| field.parse().ok())
            .ok_or_else(|| format!("expected {what}"))
    };
    let tab = number("a tab")?;
    let width = number("a width")?;
    let src = fields.next().ok_or("expected a layout")?;
    let layout = typeset::dsl::parse(src).map_err(|e| e.to_string())?;
    Ok(layout.compile().render(tab, width))
}
