# typeset-parser

**Compile-time macro parser for the typeset pretty printing library.**

This crate provides the `layout!` procedural macro that allows you to write typeset layouts using a concise DSL syntax instead of manually constructing layout trees with function calls.

## Features

- **Concise syntax** - Write layouts using operators instead of nested function calls
- **Compile-time parsing** - Zero runtime overhead, all parsing happens during compilation
- **Type-safe** - Full type checking and error reporting at compile time
- **IDE support** - Syntax highlighting and completion in supported editors

## Quick Start

Add both `typeset` and `typeset-parser` to your `Cargo.toml`:

```toml
[dependencies]
typeset = "2.0.5"
typeset-parser = "2.0.5"
```

Then use the macro in your code:

```rust
use typeset::*;
use typeset_parser::layout;

let my_layout = layout! {
    "Hello" + "World" @
    nest("Indented" + "content")
};

let doc = compile(my_layout);
println!("{}", render(doc, 2, 40));
```

## DSL Syntax Reference

### Text Literals
```rust
layout! { "Hello, World!" }
// Equivalent to: text("Hello, World!".to_string())
```

### Variables
You can reference Rust variables containing `Box<Layout>` values:
```rust
let name = text("Alice".to_string());
let greeting = layout! { "Hello" + name };
```

### Null Layout
```rust
layout! { null }
// Equivalent to: null()
```

### Composition Operators

| Operator | Name | Equivalent Function Call | Description |
|----------|------|-------------------------|-------------|
| `&` | Unpadded composition | `comp(left, right, false, false)` | Join without spaces |
| `+` | Padded composition | `comp(left, right, true, false)` | Join with spaces |
| `!&` | Fixed unpadded | `comp(left, right, false, true)` | Unpadded with infix fix |
| `!+` | Fixed padded | `comp(left, right, true, true)` | Padded with infix fix |
| `@` | Line break | `line(left, right)` | Force line break |
| `@@` | Double line break | `line(left, line(null(), right))` | Force blank line |

### Layout Constructors

| Constructor | Syntax | Equivalent Function Call | Description |
|-------------|--------|-------------------------|-------------|
| `fix` | `fix(layout)` | `fix(layout)` | Prevent breaking |
| `grp` | `grp(layout)` | `grp(layout)` | Group breaking |
| `seq` | `seq(layout)` | `seq(layout)` | Sequential breaking |
| `nest` | `nest(layout)` | `nest(layout)` | Fixed indentation |
| `pack` | `pack(layout)` | `pack(layout)` | Aligned indentation |

## Examples

### Basic Usage
```rust
use typeset_parser::layout;

// Simple composition
let basic = layout! { "Name:" + "Alice" };

// With line breaks
let multiline = layout! {
    "First line" @
    "Second line" @@
    "After blank line"
};
```

### Complex Layouts
```rust
// Function signature formatting
let params = vec![
    text("param1".to_string()),
    text("param2".to_string()),
    text("param3".to_string()),
];

let function = layout! {
    "fn" + "my_function" & "(" &
    pack(seq(params[0].clone() & "," + params[1].clone() & "," + params[2].clone())) &
    ")" + "{" @
    nest("// function body") @
    "}"
};
```

### Document Formatting
```rust
let document = layout! {
    fix("# ") & "Title" @@
    
    fix("## ") & "Section" @
    "This is a paragraph with" + "multiple words" +
    "that will break intelligently." @@
    
    fix("```") @
    nest("code example") @
    fix("```")
};
```

### JSON-like Structure
```rust
let json_object = layout! {
    "{" @
    nest(
        fix("\"name\":") + "\"Alice\"" & "," @
        fix("\"age\":") + "30" & "," @
        fix("\"city\":") + "\"New York\""
    ) @
    "}"
};
```

## Advanced Features

### Operator Precedence
The DSL respects standard operator precedence:
1. **Parentheses** `()` - highest precedence
2. **Constructors** `fix`, `grp`, `seq`, `nest`, `pack`
3. **Line breaks** `@`, `@@`
4. **Compositions** `&`, `!&`, `+`, `!+` - lowest precedence

### Parentheses for Grouping
Use parentheses to override default precedence:
```rust
layout! { 
    "prefix" + (fix("fixed" & "together")) + "suffix"
    // vs
    "prefix" + fix("fixed" & "together") + "suffix"
}
```

### Infix Fixed Operators
The `!&` and `!+` operators create infix-fixed compositions, which are syntactic sugar for fixing the boundary between two layouts:

```rust
// These are equivalent:
layout! { left !+ right }
comp(left, right, true, true)

// Useful for separators that must stay attached:
layout! { "item1" !+ "," + "item2" !+ "," + "item3" }
```

## Error Handling

The macro provides helpful compile-time error messages:

```rust
// This will produce a clear error:
layout! { unknown_operator x y }
//        ^^^^^^^^^^^^^^^^
//        Error: Expected a unary operator
```

## Integration with Manual Construction

You can freely mix DSL syntax with manual constructor calls:

```rust
let manual_part = comp(text("manual".to_string()), text("construction".to_string()), true, false);

let mixed = layout! {
    "DSL" + "part" @
    manual_part @
    "more" + "DSL"
};
```

## Performance

- **Zero runtime overhead** - All parsing happens at compile time
- **Efficient output** - Generates the same code as manual constructor calls
- **Compile-time optimization** - The macro can optimize simple cases

## Debugging

To see what code the macro generates, use `cargo expand` (requires `cargo-expand`):

```bash
cargo install cargo-expand
cargo expand --example your_example
```

This will show you the expanded Rust code that the macro produces.

## Grammar Reference

The complete grammar for the layout DSL:

```
layout ::= binary | atom

binary ::= atom operator layout

atom ::= primary | unary

unary ::= constructor primary

primary ::= variable | string | null | "(" layout ")"

operator ::= "@" | "@@" | "&" | "!&" | "+" | "!+"

constructor ::= "fix" | "grp" | "seq" | "nest" | "pack"

variable ::= IDENT

string ::= STRING_LITERAL

null ::= "null"
```

## Comparison with Manual Construction

| Manual | DSL | Notes |
|--------|-----|-------|
| `text("hello".to_string())` | `"hello"` | Much more concise |
| `comp(a, b, true, false)` | `a + b` | Clearer intent |
| `line(a, b)` | `a @ b` | More readable |
| `nest(comp(a, b, true, false))` | `nest(a + b)` | Easy nesting |
| Complex nested calls | Flat operator syntax | Much easier to read |

The DSL is especially valuable for complex layouts where manual construction becomes unwieldy.

## Contributing

This is a procedural macro crate built with:
- `syn` for parsing Rust syntax
- `quote` for generating Rust code
- `proc-macro2` for token stream manipulation

The main parsing logic is in `src/lib.rs` with a recursive descent parser for the layout DSL grammar.

## See Also

- [typeset](../typeset/) - The core pretty printing library
- [Examples](../typeset/examples/) - Comprehensive usage examples  
- [typeset documentation](https://docs.rs/typeset/) - API reference