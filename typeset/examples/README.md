# Typeset Examples

This directory contains comprehensive examples demonstrating how to use the typeset library for pretty printing various data structures and formats.

## Available Examples

### 1. `basic.rs` - Foundation Concepts
- Demonstrates all basic layout combinators
- Shows the difference between padded/unpadded compositions
- Illustrates `fix`, `grp`, `seq`, `nest`, and `pack` behaviors
- Perfect starting point for understanding typeset

**Run with:** `cargo run --example basic -p typeset`

### 2. `json_formatter.rs` - Practical JSON Pretty Printer
- Complete JSON formatter implementation
- Handles nested objects and arrays
- Demonstrates intelligent line breaking for complex data
- Shows how to build recursive formatters

**Run with:** `cargo run --example json_formatter -p typeset`

### 3. `lisp_formatter.rs` - S-Expression Pretty Printer  
- Lisp/Scheme S-expression formatter
- Demonstrates `pack()` for aligned indentation (classic Lisp style)
- Shows alternative formatting approaches (pack vs sequence)
- Great example of domain-specific formatting

**Run with:** `cargo run --example lisp_formatter -p typeset`

### 4. `code_formatter.rs` - Source Code Pretty Printer
- Formats a simple imperative programming language
- Shows complex statement and expression formatting
- Demonstrates proper block indentation and control flow layout
- Advanced example with nested structure handling

**Run with:** `cargo run --example code_formatter -p typeset`

### 5. `dsl_syntax.rs` - DSL Macro Demonstration
- Shows the concise macro syntax vs manual constructors
- Demonstrates all DSL operators (`@`, `@@`, `&`, `!&`, `+`, `!+`)
- Includes practical examples like function signatures and documents
- Perfect for learning the macro syntax

**Run with:** `cargo run --example dsl_syntax -p typeset`

### 6. `convenience_api.rs` - One-Step Formatting
- `format_layout()` and the joining/wrapping helpers (`join_with_commas`,
  `braces`, `brackets`, ...)
- The shortest path from a layout to a string

**Run with:** `cargo run --example convenience_api -p typeset`

### Profiling probes (not tutorials)
`perf_probe.rs` and `alloc_probe.rs` generate scalable workloads and print
timing and allocation counts. They are documented in
[docs/context/PERFORMANCE.md](../../docs/context/PERFORMANCE.md).

## Key Concepts Demonstrated

### Layout Combinators
- **`text()`** - Basic text literals
- **`comp()`** - Composition with `Pad` (spacing) and `Break` (breakable/fixed) options
- **`line()`** - Forced line breaks
- **`null()`** - Empty layout (identity element)

### Layout Modifiers
- **`fix()`** - Prevent breaking (treat as atomic)
- **`grp()`** - Group breaking (break as unit or not at all)
- **`seq()`** - Sequential breaking (if one breaks, all break)
- **`nest()`** - Fixed-width indentation
- **`pack()`** - Align to first element position

### DSL Operators
- **`@`** - Single line break (`line()`)
- **`@@`** - Double line break (blank line)
- **`&`** - Unpadded composition
- **`!&`** - Infix-fixed unpadded composition
- **`+`** - Padded composition
- **`!+`** - Infix-fixed padded composition

### Compile, then render
All examples follow the same shape:
1. **Construction** - Build a layout tree with combinators
2. **Compilation** - `compile()` lowers it to a `Doc` (width-independent)
3. **Rendering** - `render()` produces text at a given tab and width; a
   compiled `Doc` can be rendered at many widths

## Design Patterns

### Recursive Formatters
Most examples show how to build recursive formatters that handle nested data structures properly.

### Width-Responsive Formatting
Examples demonstrate how the same layout renders differently at various widths, showing the power of the intelligent line-breaking algorithm.

### Domain-Specific Layout
Each example shows how to adapt the general layout system to specific formatting needs (JSON, Lisp, source code, etc.).

## Learning Path

1. **Start with `basic.rs`** - Learn the fundamental combinators
2. **Try `dsl_syntax.rs`** - Learn the macro syntax  
3. **Study `json_formatter.rs`** - See a practical recursive formatter
4. **Explore `lisp_formatter.rs`** - Learn advanced alignment techniques
5. **Master `code_formatter.rs`** - Complex real-world formatting

## Tips for Building Your Own Formatters

1. **Start simple** - Begin with basic text and composition
2. **Use `grp()` for optional breaking** - Content that should break as a unit
3. **Use `seq()` for list-like data** - When one item breaks, all should break
4. **Use `pack()` for alignment** - Great for function calls and similar constructs
5. **Test at different widths** - Verify your formatter works across width ranges
6. **Leverage the DSL** - Much more concise than manual constructors for complex layouts

## Performance Notes

- The `compile()` step does the heavy lifting - you can render at multiple widths efficiently
- Use `fix()` sparingly - it prevents optimization opportunities
- The layout system is functional - you can reuse compiled documents safely