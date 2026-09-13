# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust workspace for typeset pretty printing library:
- **typeset**: the layout language, compiler, renderer, and `typeset::dsl` (runtime DSL parser)
- **typeset-parser**: the `layout!` procedural macro (compile-time DSL)
- **tests/differential**: unpublished driver the OCaml differential harness renders through

The compiler is a port of an OCaml reference implementation; every change must
render byte-identically to it (the pre-commit hook and CI run the QCheck suite
and the differential fuzzer).

## Quick Reference

### Essential Commands
```bash
# Rust tests
cargo test --all

# Differential harness against the OCaml reference (needs opam: qcheck, typeset)
cd tests && ./build.sh && ./run.sh && python3 fuzz.py 3000 1

# Fix code quality issues
./scripts/fix-code-quality.sh

# Run examples
cargo run --example <name> -p typeset
```

### Pre-commit Requirements
All commits must pass: formatting, linting, type checking, doc build, Rust tests, the differential fuzzer, and the OCaml QCheck suite. A git hook enforces these — run `./scripts/install-hooks.sh` once per clone to enable it.

## Detailed Context

For comprehensive information, see context documents in `/docs/context/`:

- **[ARCHITECTURE.md](docs/context/ARCHITECTURE.md)**: System design, core components, layout system internals
- **[DEVELOPMENT.md](docs/context/DEVELOPMENT.md)**: Build commands, testing, examples, dependencies
- **[DSL_SYNTAX.md](docs/context/DSL_SYNTAX.md)**: Complete DSL reference, operators, constructors, examples  
- **[TESTING.md](docs/context/TESTING.md)**: Test strategy, Rust + OCaml testing, running tests
- **[PERFORMANCE.md](docs/context/PERFORMANCE.md)**: Benchmarking, profiling, audit findings, optimization candidates
- **[CI_CD.md](docs/context/CI_CD.md)**: GitHub workflows, semantic versioning, release process
- **[GIT_HOOKS.md](docs/context/GIT_HOOKS.md)**: Pre-commit hooks, quality enforcement, troubleshooting