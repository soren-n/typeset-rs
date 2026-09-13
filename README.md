# typeset

[![Crates.io](https://img.shields.io/crates/v/typeset)](https://crates.io/crates/typeset)
[![docs.rs](https://img.shields.io/docsrs/typeset)](https://docs.rs/typeset)
[![CI](https://github.com/soren-n/typeset-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/soren-n/typeset-rs/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.96.0-blue)](https://github.com/soren-n/typeset-rs)

An embedded DSL for defining source code pretty printers. The layout language is designed to fit naturally over a structurally recursive pass of some inductive data-structure. A layout is a tree of text literals composed with padded/unpadded compositions or line-breaks; the solver greedily fits as many literals per line as possible while respecting grouping, sequencing, and indentation properties.

## Installation

```bash
cargo add typeset typeset-parser
```

## Quick Start

```rust
use typeset::*;

// A call whose arguments align under the first one once they break.
let args = pack(seq(join_with_commas([text("x"), text("y"), text("z")])));
let call = unpad(text("f("), unpad(args, text(")")));

// Compile once, render at any tab width and target line width.
let doc = call.compile();
assert_eq!(doc.render(2, 80), "f(x, y, z)");
assert_eq!(doc.render(2, 6), "f(x,\n  y,\n  z)");
```

The `typeset-parser` crate provides a procedural macro for more succinct layout definitions:

```rust
use typeset_parser::layout;

let my_layout = layout! {
    nest ("foo" !& "bar") @
    pack (seq ("baz" + fragment)) @@
    fix (a + b)
};
let result = my_layout.compile().render(2, 80);
```

## Crates

| Crate | Description |
|-------|-------------|
| [typeset](typeset/) | Core library: layout constructors, compiler, and renderer |
| [typeset-parser](typeset-parser/) | Procedural macro parser for the layout DSL |

## Examples

See the [examples](typeset/examples/) directory:

```bash
cargo run --example json_formatter -p typeset
cargo run --example lisp_formatter -p typeset
cargo run --example code_formatter -p typeset
cargo run --example full -p typeset-parser
```

## Documentation

- [API reference (docs.rs)](https://docs.rs/typeset)
- [DSL syntax reference](docs/context/DSL_SYNTAX.md) (compile time via `layout!`, run time via `typeset::dsl::parse`)
- [Architecture overview](docs/context/ARCHITECTURE.md)
- [Contributing guide](.github/CONTRIBUTING.md)

## License

See [LICENSE](LICENSE) for details.
