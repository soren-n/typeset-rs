# typeset

[![Crates.io](https://img.shields.io/crates/v/typeset)](https://crates.io/crates/typeset)
[![docs.rs](https://img.shields.io/docsrs/typeset)](https://docs.rs/typeset)
[![CI](https://github.com/soren-n/typeset-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/soren-n/typeset-rs/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.89.0-blue)](https://github.com/soren-n/typeset-rs)

An embedded DSL for defining source code pretty printers. The layout language is designed to fit naturally over a structurally recursive pass of some inductive data-structure. A layout is a tree of text literals composed with padded/unpadded compositions or line-breaks; the solver greedily fits as many literals per line as possible while respecting grouping, sequencing, and indentation properties.

## Installation

```bash
cargo add typeset typeset-parser
```

## Quick Start

```rust
use typeset::{compile, render, text, comp, nest, grp};

// Create a simple layout
let layout = comp(
    text("function".to_string()),
    nest(comp(
        text("name()".to_string()),
        text("{ body }".to_string()),
        true, false
    )),
    true, false
);

// Compile and render with indent width 2, buffer width 40
let doc = compile(layout);
let output = render(doc, 2, 40);
println!("{}", output);
```

The `typeset-parser` crate provides a procedural macro for more succinct layout definitions:

```rust
use typeset::{compile, render};
use typeset_parser::layout;

let my_layout = layout! {
    nest ("foo" !& "bar") @
    pack (seq ("baz" + fragment)) @@
    fix (a + b)
};

let doc = compile(my_layout);
let result = render(doc, 2, 80);
```

## Crates

| Crate | Description |
|-------|-------------|
| [typeset](typeset/) | Core library: layout constructors, compiler, and renderer |
| [typeset-parser](typeset-parser/) | Procedural macro parser for the layout DSL |

## Examples

See the [examples](typeset/examples/) directory:

```bash
cargo run --example basic -p typeset
cargo run --example json_formatter -p typeset
cargo run --example code_formatter -p typeset
cargo run --example full -p typeset-parser
```

## Documentation

- [API reference (docs.rs)](https://docs.rs/typeset)
- [DSL syntax reference](docs/context/DSL_SYNTAX.md)
- [Architecture overview](docs/context/ARCHITECTURE.md)
- [Contributing guide](.github/CONTRIBUTING.md)

## License

See [LICENSE](LICENSE) for details.
