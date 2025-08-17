# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust workspace containing two main crates:
- **typeset**: A DSL for defining source code pretty printers
- **typeset-parser**: A procedural macro parser that provides compile-time DSL parsing for typeset

## Architecture

### Core Components

**typeset crate** (`typeset/src/`):
- `lib.rs`: Main public API exports (Layout, Doc, constructors, compile, render)
- `compiler.rs`: Core layout compiler and renderer implementation
- `avl.rs`, `list.rs`, `map.rs`: Persistent data structures for layout processing
- `order.rs`, `util.rs`: Supporting utilities

**typeset-parser crate** (`typeset-parser/src/`):
- `lib.rs`: Procedural macro implementation for parsing layout DSL syntax
- Uses `syn`, `quote`, `proc-macro2` for macro parsing

### Layout System Architecture

The library implements a two-phase pretty printing system:
1. **Layout Construction**: Build layout trees using constructors (text, comp, nest, fix, grp, seq, pack)
2. **Compilation & Rendering**: `compile()` optimizes layouts, `render()` outputs text

Key layout concepts:
- Compositions can be padded/unpadded and fixed/breakable
- Special constructors: `fix` (treat as literal), `grp` (break as group), `seq` (break all if one breaks)
- Indentation: `nest` (fixed width), `pack` (align to first literal position)

## Development Commands

### Building
```bash
cargo build                    # Build all workspace members
cargo build -p typeset        # Build specific crate
cargo build -p typeset-parser
```

### Testing
```bash
cd tests && ./build.sh        # Build test infrastructure (OCaml tester + Rust unit tests)
cd tests && ./run.sh          # Run all tests
```

The test system uses:
- OCaml-based tester in `tests/tester/` (requires opam, dune, qcheck, typeset)
- Rust unit tests in `tests/unit/`
- Build script compiles both and places executables in `tests/_build/`

### Examples
```bash
cargo run --example <name> -p typeset-parser    # Run parser examples
```

## Key Dependencies

- **typeset**: `bumpalo` for bump allocation
- **typeset-parser**: `syn`, `quote`, `proc-macro2` for procedural macros

## DSL Syntax

The parser supports layout DSL with operators:
- `@` / `@@`: forced line breaks
- `&` / `!&`: unpadded compositions (with/without infix fix)
- `+` / `!+`: padded compositions (with/without infix fix)
- Constructors: `fix`, `grp`, `seq`, `nest`, `pack`, `null`

## Git Hooks

Pre-commit hooks are configured to enforce code quality:
- **Formatting**: `cargo fmt --check` (must pass)
- **Linting**: `cargo clippy` (warnings allowed, errors blocked)
- **Type checking**: `cargo check --all-targets --all-features`
- **Rust testing**: `cargo test --all --all-features`
- **OCaml testing**: Installs dependencies (qcheck, typeset) if missing, then runs both Rust and OCaml tests

### Prerequisites for OCaml Tests
- OCaml and opam must be installed
- Git hooks will auto-install missing OCaml packages: `qcheck`, `typeset`

To fix formatting and linting issues quickly:
```bash
./fix-code-quality.sh    # Auto-fix formatting and clippy issues
```

All checks (including both Rust and OCaml tests) must pass before commits are allowed.

## CI/CD Pipeline

### GitHub Workflows

**CI Pipeline** (`.github/workflows/ci.yml`) - Runs on every push and PR:
- Quality gates: formatting, linting, type checking
- Matrix testing: stable and nightly Rust
- OCaml property-based testing 
- Security auditing with cargo-audit and cargo-deny
- Build verification and artifact generation

**Release Pipeline** (`.github/workflows/release.yml`) - Automated semantic versioning:
- Analyzes conventional commits to determine version bump
- Updates Cargo.toml versions automatically
- Generates CHANGELOG.md from commit messages
- Creates git tags and GitHub releases
- Publishes crates to crates.io in dependency order

**Dependencies** (`.github/workflows/dependencies.yml`) - Weekly maintenance:
- Updates Rust dependencies with automated PRs
- Security vulnerability scanning
- License compliance checking

### Semantic Versioning

Uses [Conventional Commits](https://conventionalcommits.org/) for automatic versioning:
- `feat:` → minor version bump
- `fix:` → patch version bump  
- `BREAKING CHANGE:` or `!` → major version bump

### Development Workflow
1. Create feature branch with descriptive name
2. Make commits following conventional commit format
3. Push triggers CI validation
4. Create PR for review
5. Merge to main triggers release pipeline (if applicable)
6. Automated version bump, changelog, and crate publishing