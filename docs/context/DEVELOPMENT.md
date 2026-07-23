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
- Unit tests: `typeset/tests/` and inline `#[cfg(test)]` modules
- Integration tests: `tests/unit/` (separate crate)
- Performance tests: `typeset/benches/`

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
- `basic.rs`: Simple layout construction
- `dsl_syntax.rs`: DSL syntax demonstration  
- `json_formatter.rs`: JSON pretty printer
- `lisp_formatter.rs`: Lisp-style formatter
- `code_formatter.rs`: Source code formatting
- `convenience_api.rs`: High-level API usage

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
- **typeset**: `bumpalo` for bump allocation during compilation
- **typeset-parser**: `syn`, `quote`, `proc-macro2` for procedural macros

### Development Dependencies
- `criterion`: Benchmarking framework
- Various test utilities in dev-dependencies sections

## Project Layout Standards

- Use existing code style and conventions
- Follow Rust naming conventions (snake_case for functions, PascalCase for types)
- Never recurse on the native stack over user-controlled depth. Prefer
  eliminating the recursion structurally: intermediate representations are flat
  postorder arenas (children precede parents), so bottom-up folds are forward
  loops and inherited context is a backward loop — no frame stacks at all. The
  output `Doc` is a flat arena too, so `Clone`/`Drop`/`Debug` are derived and
  deep-safe. The one `Box`-recursive tree is the public `Layout` input, which
  keeps hand-written iterative `Clone`/`Drop`/`Debug` and is walked exactly
  once, by the `flatten` pass
- Keep new intermediate state flat and owned; the single bump arena backs
  `serialize`'s persistent scope accumulators and should stay the only one
- Maintain separation between layout construction and compilation phases