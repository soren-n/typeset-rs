# Development Guide

## Building

### Basic Build Commands
```bash
cargo build                    # Build all workspace members
cargo build -p typeset        # Build specific crate
cargo build -p typeset-parser
```

### Release Builds
```bash
cargo build --release         # Optimized builds
cargo build --release -p typeset
```

## Testing

### Quick Test Setup
```bash
cd tests && ./build.sh        # Build test infrastructure (OCaml tester + Rust unit tests)
cd tests && ./run.sh          # Run all tests
```

### Test System Architecture

**Rust Tests**:
- Unit tests: inline `#[cfg(test)]` modules per pass
- `typeset/tests/rendering.rs`: exact-output tests whose expected strings
  came from the OCaml oracle; `scaling.rs`: the linear-time guard;
  `unicode_width_tests.rs`
- Differential driver: `tests/differential/` (workspace bin, not published)
- Benchmarks: `typeset/benches/`

**OCaml Property-Based Tests**:
- Located in: `tests/tester/`
- Requires: opam, dune, qcheck, typeset OCaml package
- Validates layout behavior against reference OCaml implementation
- Build script: compiles both systems, places executables in `tests/_build/`

### Individual Test Commands
```bash
# Rust only
cargo test --all --all-features

# OCaml only (requires setup)
cd tests/tester && dune exec ./bin/main.exe

# Benchmarks (small-input latency + asymptotic scaling suites)
cargo bench -p typeset --bench layout_performance
cargo bench -p typeset --bench scaling
```

Profiling probes (`examples/perf_probe.rs`, `examples/alloc_probe.rs`) and the
full benchmarking/profiling guide live in
[PERFORMANCE.md](PERFORMANCE.md).

## Examples

### Running Examples
```bash
cargo run --example <name> -p typeset-parser    # Parser examples
cargo run --example <name> -p typeset          # Layout examples
```

### Available Examples
`typeset`:
- `basic.rs`: Simple layout construction
- `dsl_syntax.rs`: DSL syntax demonstration
- `json_formatter.rs`: JSON pretty printer
- `lisp_formatter.rs`: Lisp-style formatter
- `code_formatter.rs`: Source code formatting
- `convenience_api.rs`: High-level API usage
- `perf_probe.rs`, `alloc_probe.rs`: profiling probes (see PERFORMANCE.md)

`typeset-parser`:
- `full.rs`: every `layout!` operator and constructor in one macro invocation

## Code Quality

### Automated Fixes
```bash
./scripts/fix-code-quality.sh    # Auto-fix formatting and clippy issues
```

### Manual Quality Checks
```bash
cargo fmt --check              # Check formatting
cargo clippy --all-targets     # Run linter
cargo check --all-targets --all-features  # Type checking
```

## Key Dependencies

### Runtime Dependencies
- **typeset**: none (the compiler is self-contained — only the standard
  library; the former `bumpalo` bump allocator has been retired)
- **typeset-parser**: `syn`, `quote`, `proc-macro2` for procedural macros

### Development Dependencies
- `criterion`: Benchmarking framework
- `typeset` and `typeset-parser` dev-depend on each other (doctests and
  examples); the release workflow strips the parser's side before publishing

## Project Layout Standards

- Use existing code style and conventions
- Follow Rust naming conventions (snake_case for functions, PascalCase for types)
- Never recurse on the native stack over user-controlled depth. Every
  representation, `Layout` and `Doc` included, is a flat postorder arena
  (children precede parents), so bottom-up folds are forward loops, inherited
  context is a backward loop, and `Clone`/`Drop`/`Debug` derive. Where a walk
  needs a stack (the DFS in `serialize`, the renderer, the DSL parser), it is
  an explicit `Vec` of frames
- Use the arena primitives in `types/arena.rs`: `Arena<T>` with typed
  `Id<T>`s, `IdVec<K, V>` side tables, `Range<T>` into shared buffers, and
  `Option<Id<T>>` for absent links — never raw `u32` indices or sentinel values
- A pass owns the type it produces; only genuinely shared vocabulary goes in
  `types/ir.rs`
- Every change is held to byte-identical output against the OCaml oracle
  (`cd tests && ./build.sh && python3 fuzz.py 3000 1`)