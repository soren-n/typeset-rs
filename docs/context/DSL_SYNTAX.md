# DSL Syntax Reference

## Overview

The `typeset-parser` crate provides the `layout! { ... }` procedural macro. It
parses a compact layout DSL at compile time and expands it into calls to the
`typeset` constructor functions, so the two are always semantically equivalent —
the macro is pure sugar.

```rust
use typeset_parser::layout;

let name = typeset::text("name");
let doc = layout! {
    "fn" + name @
    grp ("(" & "arg" & ")")
};
```

## Operands (primaries)

A primary is the smallest unit an operator applies to:

- `"literal"` — a string literal, expands to `text("literal")`. Write the string
  directly; do **not** write `text("literal")` inside the macro.
- `null` — the empty layout, expands to `null()`.
- `identifier` — any bare identifier that is not `null` or a unary operator is
  treated as a variable holding a `Box<Layout>` in the surrounding scope; it
  expands to `identifier.clone()`.
- `( expr )` — a parenthesized sub-expression, used for grouping.

## Unary operators (prefix)

Each is written as the operator name followed by a single primary (usually a
parenthesized expression):

- `fix (layout)` — never break anything inside `layout`.
- `grp (layout)` — group breaking: every breakable composition inside breaks
  together, all-or-nothing.
- `seq (layout)` — sequential breaking: once one composition breaks, every later
  one in the sequence breaks too.
- `nest (layout)` — indent the lines `layout` breaks onto by the render `tab`
  width. `nest` takes only the layout; the indent amount is the `tab` argument
  passed to `render`, not a macro argument.
- `pack (layout)` — align the lines `layout` breaks onto to the column where its
  first element started (hanging indentation).

## Binary operators (infix)

The composition operators produce breakable or fixed compositions; the line
operators force breaks:

| Operator | Expands to        | Space? | Breaks?                        |
|----------|-------------------|--------|--------------------------------|
| `&`      | `unpad(l, r)`     | no     | may break if the line is long  |
| `+`      | `pad(l, r)`       | yes    | may break if the line is long  |
| `!&`     | `fix_unpad(l, r)` | no     | never breaks                   |
| `!+`     | `fix_pad(l, r)`   | yes    | never breaks                   |
| `@`      | `line(l, r)`      | —      | always breaks (hard newline)   |
| `@@`     | `line(l, line(null, r))` | — | always breaks, leaving a blank line between (paragraph break) |

So the soft, fit-dependent breaks are the compositions (`&`, `+`); `@` and `@@`
are unconditional line breaks.

## Precedence and associativity

All binary operators share a **single precedence level** and are **right
associative**. `a + b + c` parses as `a + (b + c)`, and `a + b & c` parses as
`a + (b & c)`. Use parentheses to impose any other grouping — there is no
operator-precedence hierarchy, so parenthesize whenever the intended structure
is not a simple right-leaning chain.

## Examples

```rust
use typeset_parser::layout;

// A variable holding a Box<Layout> is referenced by bare name.
let body = typeset::text("body");

// Padded composition, a hard break, then an indented group.
let block = layout! {
    "fn" + "name()" @
    "{" @@
    nest (body) @@
    "}"
};

// Unpadded call syntax, breakable argument group.
let call = layout! {
    "f" & grp ("(" & "a" & "," + "b" & ")")
};

// Right associativity means the chain leans right; parenthesize to regroup.
let regrouped = layout! { ("a" + "b") + "c" };
```

## Parser implementation notes

- Built on the `syn` crate; primaries, unary operators, and infix operators are
  parsed by ordered speculative alternatives (`parse_any`).
- Each node expands directly to the matching named constructor
  (`unpad`/`pad`/`fix_unpad`/`fix_pad`/`line`/`null`/`text`/`fix`/`grp`/`seq`/`nest`/`pack`),
  so the macro never re-derives raw composition booleans.
- Errors carry span information for editor integration.
