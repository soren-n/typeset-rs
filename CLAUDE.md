# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust workspace for typeset pretty printing library:
- **typeset**: the layout language, compiler, renderer, and `typeset::dsl` (runtime DSL parser)
- **typeset-parser**: the `layout!` procedural macro (compile-time DSL)
- **oracle/driver**: unpublished driver the OCaml oracle harness renders through

The compiler is a port of an OCaml reference implementation; every change must
render byte-identically to it (the pre-commit hook and CI run the QCheck
identity suite in `oracle/`).

## Quick Reference

### Essential Commands
```bash
# Rust tests
cargo test --all

# Oracle harness against the OCaml reference (needs opam: dune, qcheck, typeset)
cd oracle && ./build.sh && ./_build/tester
./compare.sh '"a" + grp ("b" + "c")' 2 3    # one expression, both implementations

# Fix code quality issues
./scripts/fix-code-quality.sh

# Run examples
cargo run --example <name> -p typeset
```

### Pre-commit Requirements
All commits must pass: formatting, linting, type checking, doc build, Rust tests, and the OCaml QCheck identity suite. A git hook enforces these — run `./scripts/install-hooks.sh` once per clone to enable it.

## Detailed Context

For comprehensive information, see context documents in `/docs/context/`:

- **[ARCHITECTURE.md](docs/context/ARCHITECTURE.md)**: System design, core components, layout system internals
- **[DEVELOPMENT.md](docs/context/DEVELOPMENT.md)**: Build commands, testing, examples, dependencies
- **[DSL_SYNTAX.md](docs/context/DSL_SYNTAX.md)**: Complete DSL reference, operators, constructors, examples  
- **[TESTING.md](docs/context/TESTING.md)**: Test strategy, Rust + OCaml testing, running tests
- **[PERFORMANCE.md](docs/context/PERFORMANCE.md)**: Benchmarking, profiling, audit findings, optimization candidates
- **[CI_CD.md](docs/context/CI_CD.md)**: GitHub workflows, semantic versioning, release process
- **[GIT_HOOKS.md](docs/context/GIT_HOOKS.md)**: Pre-commit hooks, quality enforcement, troubleshooting