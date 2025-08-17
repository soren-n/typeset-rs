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
- Output executables placed in `tests/_build/`
- Handles dependency installation for OCaml components

### OCaml Setup Requirements
- **System**: OCaml and opam must be installed
- **Packages**: Auto-installed by git hooks if missing
  - `qcheck`: Property-based testing framework
  - `typeset`: Reference implementation for comparison

### Git Hook Integration
Pre-commit hooks automatically run the complete test suite:
- Installs missing OCaml dependencies if needed
- Runs both Rust and OCaml tests
- Blocks commits if any tests fail

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
- Every commit (via git hooks)
- Pull requests (via GitHub Actions)  
- Multiple Rust versions (stable, nightly)
- Security auditing with additional tools

All tests must pass before code can be merged to main branch.