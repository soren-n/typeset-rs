# DSL Syntax Reference

## Overview

The typeset-parser crate provides a procedural macro that parses layout DSL syntax at compile time, converting it into layout constructor calls.

## Operators

### Line Break Operators
- `@`: Soft line break - break here if the line becomes too long
- `@@`: Hard line break - always break here

### Composition Operators

#### Unpadded Compositions
- `&`: Basic unpadded composition (no spaces between elements)
- `!&`: Unpadded composition with infix fix (prevents breaks within)

#### Padded Compositions  
- `+`: Basic padded composition (spaces between elements)
- `!+`: Padded composition with infix fix (prevents breaks within)

## Constructors

### Basic Constructors
- `text("string")`: Literal text node
- `null`: Empty layout (identity element)

### Control Constructors
- `fix(layout)`: Treat layout as literal text (no line breaks allowed)
- `grp(layout)`: Group breaking - all elements break together or not at all
- `seq(layout)`: Sequential breaking - if any element breaks, all break

### Indentation Constructors
- `nest(n, layout)`: Increase indentation by `n` spaces for nested content
- `pack(layout)`: Align content to the column position of the first literal

## Syntax Examples

### Basic Text and Composition
```rust
// Simple text
text("hello")

// Padded composition with space
text("hello") + text("world")  // "hello world"

// Unpadded composition  
text("(") & text("content") & text(")")  // "(content)"
```

### Line Breaking
```rust
// Soft break - only breaks if needed
text("item1") @ text("item2")

// Hard break - always breaks
text("line1") @@ text("line2")
```

### Grouping and Control
```rust
// Group breaking - all break together
grp(text("a") + text("b") + text("c"))

// Fixed content - no breaks allowed
fix(text("no") + text("breaks"))

// Sequential breaking
seq(text("first") @ text("second") @ text("third"))
```

### Indentation
```rust
// Fixed indentation increase
text("parent") @@ nest(2, text("child"))

// Pack to first literal position
text("fn") + pack(text("name") @ text("body"))
```

### Complex Examples
```rust
// Function definition style
text("fn") + text("name") & text("(") &
  nest(2, text("param1") & text(",") @ text("param2")) &
  text(")") @
  text("{") @@ 
  nest(2, text("body")) @@
  text("}")

// JSON-like structure
text("{") @@
  nest(2, 
    text("\"key\":") + text("\"value\"") & text(",") @
    text("\"key2\":") + text("\"value2\"")
  ) @@
  text("}")
```

## Precedence and Associativity

Operators follow standard precedence rules:
1. Constructors (highest)
2. `&`, `!&` (unpadded compositions)
3. `+`, `!+` (padded compositions)  
4. `@` (soft breaks)
5. `@@` (hard breaks, lowest)

Left associative: `a + b + c` parses as `(a + b) + c`

## Parser Implementation Notes

- Built using `syn` crate for robust Rust syntax parsing
- Generates constructor function calls at compile time
- Supports nested parentheses and complex expressions
- Error reporting includes span information for IDE integration