# Testing Strategy

## Test Architecture

The project uses a comprehensive dual-language testing approach combining Rust unit/integration tests with OCaml property-based tests.

## Test Categories

### 1. Rust Unit Tests
**Location**: Throughout crate modules with `#[cfg(test)]`

**Coverage**:
- Individual function behavior
- Data structure operations (the persistent `List`)
- Layout constructor behavior
- Compiler pass validation

`list.rs` carries `proptest` property tests (in `#[cfg(test)] mod proptests`)
that model the structure against a std reference — `List` against `Vec` — and
check every public operation. Integer-keyed maps now use standard-library
collections (`HashMap` for the renderer's marks, `BTreeMap` for `structurize`'s
property map), so they need no bespoke tests; the compiler's use of them is
covered end-to-end by the differential fuzzer and the OCaml oracle.

**Note**: "property-based testing" in this project refers to two independent
mechanisms — these Rust `proptest` tests over the data structures, and the
OCaml QCheck suite (below) that validates rendering against the reference.

### 2. Rust Integration Tests  
**Location**: `typeset/tests/` and `tests/unit/`

**Coverage**:
- End-to-end layout compilation and rendering
- API usage scenarios
- Parser macro functionality
- Performance characteristics

### 3. OCaml Property-Based Tests
**Location**: `tests/tester/`

**Purpose**:
- Cross-language validation against reference OCaml typeset implementation
- Property-based testing using QCheck framework
- Ensures behavioral compatibility

**Known coverage gap**: the QCheck generator picks constructors close to
uniformly, so it almost never produces the stacked `grp`/`seq` nestings where
breaking decisions actually diverge. The historical `grp(seq(x))` map-ordering
bug survived 15 consecutive clean runs of this suite. Passing runs here are not
evidence that breaking semantics are correct — use the differential tools below
for that.

### 4. Differential Tools

**Location**: `tests/compare.sh`, `tests/fuzz.py`, `tests/tester/bin/oracle.ml`

`oracle.ml` parses the same DSL grammar as `tests/unit` (see
`tests/unit/src/layout.pest`) and renders it through the OCaml reference, so a
single expression can be compared directly instead of waiting for the generator
to stumble onto it. Both wrappers expect to run from `tests/` after `./build.sh`.

```bash
cd tests
./compare.sh 'grp (seq ("a" + ("b" + "c")))' 2 3   # one expression: expr, tab, width
python3 fuzz.py 2000 7                              # iterations, seed
```

`fuzz.py` biases generation toward stacked `grp`/`seq` and renders at narrow
widths, which is what makes breaking divergences show up. It exits non-zero and
prints both renderings on the first mismatches.

Both the reference and Rust binaries take optional `tab` and `width` arguments
(defaulting to 2 and 80), so a case can be minimized by shrinking the width
rather than padding the input with long strings.

## Running Tests

### Complete Test Suite
```bash
cd tests && ./build.sh        # Build both Rust and OCaml test infrastructure  
cd tests && ./run.sh          # Run all tests
```

### Rust Tests Only
```bash
cargo test --all --all-features
```

### OCaml Tests Only
```bash
cd tests/tester && dune exec ./bin/main.exe
```

### Performance Tests
```bash
cargo bench -p typeset
```

## Test Infrastructure

### Build System
- `tests/build.sh`: Compiles both Rust unit tests and OCaml tester
- Output executables placed in `tests/_build/` (`tester`, `oracle`, `unit`)
- Does a clean rebuild each time (`dune clean` + `cargo clean`)

### OCaml Setup Requirements
- **System**: OCaml and opam must be installed
- **Packages**: install manually before the first run:
  ```bash
  opam install qcheck typeset
  ```
  - `qcheck`: Property-based testing framework
  - `typeset`: Reference implementation for comparison

Without these, `tests/build.sh` fails with `Library "qcheck" not found`.

## Test Development Guidelines

### Writing Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_behavior() {
        let layout = text("hello");
        let compiled = compile(layout);
        assert_eq!(render(compiled, 80), "hello");
    }
}
```

### Property-Based Test Concepts
The OCaml tests validate properties like:
- Layout rendering is deterministic
- Composition operators behave consistently
- Indentation rules are preserved
- Line breaking follows expected patterns

### Performance Test Guidelines
- Use `criterion` benchmarking framework
- Focus on compilation and rendering performance
- Test with various layout complexities
- Compare different data structure implementations

## Continuous Integration

Tests run automatically on:
- Every commit (via git hooks, once installed with `./scripts/install-hooks.sh`)
- Pull requests (via GitHub Actions) — Rust tests only
- Multiple Rust versions (stable, MSRV 1.89.0)
- Security auditing with additional tools

Note: the OCaml property tests are not part of CI. They run only through the
pre-commit hook, so a contributor without OCaml installed will never exercise
them.

All tests must pass before code can be merged to main branch.