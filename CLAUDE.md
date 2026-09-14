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
./_build/tester '"a" + grp ("b" + "c")' 2 3    # one expression, both implementations

# Run examples; profiling probes are harness-less benches
cargo run --example <name> -p typeset
cargo bench -p typeset --bench perf_probe -- json 8 d=5
```

### Pre-commit Requirements
All commits must pass: formatting, linting, type checking, doc build, Rust tests, and the OCaml QCheck identity suite. A git hook enforces these — run `./scripts/install-hooks.sh` once per clone to enable it.

## Detailed Context

For comprehensive information, see context documents in `/docs/context/`:

- **[ARCHITECTURE.md](docs/context/ARCHITECTURE.md)**: the arenas, the two passes, the renderer, the semantics worth knowing, the reference
- **[DEVELOPMENT.md](docs/context/DEVELOPMENT.md)**: building, the oracle harness, code standards, the pre-commit hook, CI, releasing, benchmarking and profiling

The DSL grammar is documented where it is implemented: the `typeset::dsl`
module doc (`typeset/src/dsl.rs`) and the `typeset-parser` README, which is
that crate's documentation.
