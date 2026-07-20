# Testing Strategy

## Test Architecture

The project uses a comprehensive dual-language testing approach combining Rust unit/integration tests with OCaml property-based tests.

## Test Categories

### 1. Rust Unit Tests
**Location**: Throughout crate modules with `#[cfg(test)]`

**Coverage**:
- Individual function behavior
- Data structure operations (AVL trees, lists, maps)
- Layout constructor behavior
- Compiler pass validation

Two flavors of data-structure test live in `avl.rs`, `list.rs`, and `map.rs`:

- Hand-rolled invariant tests (`check_all`) over fixed ascending / descending /
  shuffled inputs, asserting the full AVL invariants including strict balance.
- `proptest` property tests (in `#[cfg(test)] mod proptests`) that model each
  structure against a std reference — `List` against `Vec`, `Map`/`Avl` against
  `BTreeMap`/`BTreeSet` — and check every public operation. This is the
  root-cause guard for the class of bug `to_list` was: the earlier invariant
  tests never exercised the "complete implementation" functions against a model,
  so a wrong-order `to_list` and a corrupting `remove` both passed unnoticed.

  The adversarial proptests assert the structural contract (exact heights,
  counts, sorted order, contents, membership) via `check_structural`, not strict
  AVL balance: this functional-AVL port does not guarantee balance for every
  insertion/removal order (a performance-only property; ordering and contents
  stay correct). Strict balance is still asserted by the hand-rolled tests for
  representative inputs. Run more cases with `PROPTEST_CASES=4096 cargo test`.

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
breaking decisions actually diverge. The `grp(seq(x))` bug fixed in `to_list`
survived 15 consecutive clean runs of this suite. Passing runs here are not
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