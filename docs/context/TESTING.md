# Testing Strategy

## Test Architecture

The project uses a comprehensive dual-language testing approach combining Rust unit/integration tests with OCaml property-based tests.

## Test Categories

### 1. Rust Unit Tests
**Location**: `#[cfg(test)]` modules in each pass, `dsl.rs`, and `layout.rs`

**Coverage**:
- Each pass's rules on small hand-built inputs (serialize's line splitting,
  scope deltas and fix coalescing; resolve_scopes' scope reconstruction and
  widening; denull's pad merging; normalize's eliminations and reassociation;
  rescope's prefix factoring), plus the DSL parser
- Two end-to-end depth-50k tests in `lib.rs` guard constant native stack
  for the whole pipeline; passes do not repeat that individually

The compiler uses only standard-library collections (`Vec`/slices,
`BTreeMap`), so there is no bespoke data-structure test suite; their use is
covered end-to-end by the differential fuzzer and the OCaml oracle.

**Note**: "property-based testing" in this project now refers to the OCaml
QCheck suite (below) that validates rendering against the reference, together
with the Python differential fuzzer. (Earlier bespoke `proptest` tests over the
custom data structures were removed along with those structures.)

### 2. Rust Integration Tests
**Location**: `typeset/tests/`

- `rendering.rs`: exact-output tests. Every expected string was produced by
  the OCaml oracle for the same layout, tab and width; add new cases the same
  way (`oracle/_build/oracle '<dsl>' <tab> <width>`), never by pasting what the
  Rust implementation printed
- `scaling.rs`: the one wall-clock assertion, an order-of-magnitude guard
  that nested scopes compile in linear time
- `unicode_width_tests.rs`: widths are counted in characters

### 3. OCaml Property-Based Tests
**Location**: `oracle/tester/`

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

**Location**: `oracle/compare.sh`, `tests/fuzz.py`, `oracle/tester/bin/oracle.ml`

`oracle.ml` parses the same DSL grammar as `typeset::dsl` and renders it
through the OCaml reference; `oracle/driver` is the Rust driver, so a
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
cd oracle && ./build.sh        # Build both Rust and OCaml test infrastructure  
cd oracle && ./run.sh          # Run all tests
```

### Rust Tests Only
```bash
cargo test --all --all-features
```

### OCaml Tests Only
```bash
cd oracle/tester && dune exec ./bin/main.exe
```

### Performance Tests
```bash
cargo bench -p typeset            # both suites
cargo bench -p typeset --bench scaling   # asymptotics only
```
See [PERFORMANCE.md](PERFORMANCE.md) for the benchmarking/profiling guide.

## Test Infrastructure

### Build System
- `oracle/build.sh`: Compiles both Rust unit tests and OCaml tester
- Output executables placed in `oracle/_build/` (`tester`, `oracle`, `unit`)
- Does a clean rebuild each time (`dune clean` + `cargo clean`)

### OCaml Setup Requirements
- **System**: OCaml and opam must be installed
- **Packages**: install manually before the first run:
  ```bash
  opam install qcheck typeset
  ```
  - `qcheck`: Property-based testing framework
  - `typeset`: Reference implementation for comparison

Without these, `oracle/build.sh` fails with `Library "qcheck" not found`.

## Test Development Guidelines

### Writing Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_behavior() {
        assert_eq!(text("hello").compile().render(2, 80), "hello");
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
- Guard asymptotic behavior (e.g. the linear-scaling nested-scope test), not
  wall-clock thresholds

## Continuous Integration

Tests run automatically on:
- Every commit (via git hooks, once installed with `./scripts/install-hooks.sh`)
- Pull requests and pushes to `main` (via GitHub Actions)
- Multiple Rust versions (stable, MSRV 1.96.0)
- License and advisory policy (`cargo deny`)

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs two jobs that gate
merges: a `check` job (fmt, clippy, `cargo doc`, `cargo check`, and
`cargo test --all`) and a dedicated `differential` job that installs OCaml +
opam, builds the oracle, and runs the QCheck property suite plus the
differential fuzzer. So the OCaml oracle is a first-class CI gate, not only a
local pre-commit step — a contributor without OCaml locally still has their
changes exercised against the oracle in CI.

All tests must pass before code can be merged to main branch.