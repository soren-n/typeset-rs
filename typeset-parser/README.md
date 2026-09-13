# typeset-parser

The `layout!` procedural macro: the [typeset](../typeset/) layout DSL,
parsed at compile time and expanded to `typeset` constructor calls.

```toml
[dependencies]
typeset = "5"
typeset-parser = "5"
```

```rust
use typeset::{compile, render, text};
use typeset_parser::layout;

let name = text("Alice");
let doc = compile(layout! {
    "Hello" + name @
    nest ("Indented" + "content")
});
println!("{}", render(&doc, 2, 40));
```

The same language is available at run time as `typeset::dsl::parse`, minus
variables.

## Syntax

A primary is a string literal (`text(...)`), `null`, a bare identifier (a
`Layout` variable in scope, cloned), or a parenthesized expression. A unary
operator applies to the primary that follows it:

| Unary  | Expands to | Meaning |
|--------|------------|---------|
| `fix`  | `fix(u)`   | never break inside |
| `grp`  | `grp(u)`   | compositions inside break all-or-nothing |
| `seq`  | `seq(u)`   | once one composition breaks, all later ones do |
| `nest` | `nest(u)`  | continuation lines indent by one tab |
| `pack` | `pack(u)`  | continuation lines align to the first element's column |

Binary operators:

| Operator | Expands to               | Meaning |
|----------|--------------------------|---------|
| `&`      | `unpad(l, r)`            | join without a space; may break |
| `+`      | `pad(l, r)`              | join with a space; may break |
| `!&`     | `fix_unpad(l, r)`        | join without a space; never breaks |
| `!+`     | `fix_pad(l, r)`          | join with a space; never breaks |
| `@`      | `line(l, r)`             | hard line break |
| `@@`     | `line(l, line(null(), r))` | hard break leaving a blank line |

All binary operators share **one precedence level** and associate **to the
right**: `a + b & c` is `a + (b & c)`, and `"a" + "b" @ "c"` is
`"a" + ("b" @ "c")`. Parenthesize for any other grouping.

```rust
let params = vec![text("x"), text("y")];
let call = layout! {
    "f" & "(" & pack (seq (params[0].clone() & "," + params[1].clone())) & ")"
};
```

Grammar:

```text
expr    := atom (binop expr)?
atom    := (fix | grp | seq | nest | pack)? primary
primary := IDENT | STRING | null | "(" expr ")"
binop   := & | + | !& | !+ | @ | @@
```

## Errors

Parse failures are compile errors with the span of the offending token, for
example `Expected a unary operator` on an unknown identifier in operator
position.

## Debugging

`cargo expand` shows the constructor calls a `layout!` expands to.

## See also

- [DSL syntax reference](../docs/context/DSL_SYNTAX.md)
- [typeset](../typeset/) and its [examples](../typeset/examples/)
