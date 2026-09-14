# typeset-parser

The `layout!` procedural macro: the [typeset](https://docs.rs/typeset)
layout DSL, parsed at compile time and expanded to `typeset` constructor
calls. A bare identifier is a variable: a `Layout` in scope, cloned.

```rust
use typeset::text;
use typeset_parser::layout;

let name = text("Alice");
let layout = layout! {
    "Hello" + name @
    nest ("Indented" + "content")
};
assert_eq!(layout.compile().render(2, 40), "Hello Alice\n  Indented content");
```

The grammar has one implementation, `typeset::dsl`, which also parses the
same language from a string at run time (`Layout`'s `FromStr`, minus
variables). The macro feeds it Rust tokens, so the two cannot disagree.

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
use typeset::text;
use typeset_parser::layout;

let params = vec![text("x"), text("y")];
let (x, y) = (params[0].clone(), params[1].clone());
let call = layout! {
    "f" & "(" & pack (seq (x !& "," + y)) & ")"
};
let doc = call.compile();
assert_eq!(doc.render(2, 80), "f(x, y)");
assert_eq!(doc.render(2, 4), "f(x,\n  y)");
```

Grammar:

```text
expr    := atom (binop expr)?
atom    := (fix | grp | seq | nest | pack)? primary
primary := IDENT | STRING | null | "(" expr ")"
binop   := & | + | !& | !+ | @ | @@
```

String literals are read by the DSL's own string syntax, not Rust's: the
escapes `\n \r \t \0 \\ \" \'` are accepted; raw strings, byte strings and
other Rust escapes are compile errors.

## Errors

Parse failures are compile errors at the span of the offending token, for
example `expected an operator` on two adjacent primaries.

## Debugging

`cargo expand` shows the constructor calls a `layout!` expands to.
