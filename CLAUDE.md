# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust workspace for typeset pretty printing library:
- **typeset**: DSL for defining source code pretty printers
- **typeset-parser**: Procedural macro parser for compile-time DSL parsing

## Quick Reference

### Essential Commands
```bash
# Build and test
cargo build && cd tests && ./run.sh

# Fix code quality issues  
./scripts/fix-code-quality.sh

# Run examples
cargo run --example <name> -p typeset-parser
```

### Pre-commit Requirements
All commits must pass: formatting, linting, type checking, Rust tests, and OCaml property-based tests. Git hooks enforce these automatically.

## Detailed Context

For comprehensive information, see context documents in `/docs/context/`:

- **[ARCHITECTURE.md](docs/context/ARCHITECTURE.md)**: System design, core components, layout system internals
- **[DEVELOPMENT.md](docs/context/DEVELOPMENT.md)**: Build commands, testing, examples, dependencies
- **[DSL_SYNTAX.md](docs/context/DSL_SYNTAX.md)**: Complete DSL reference, operators, constructors, examples  
- **[TESTING.md](docs/context/TESTING.md)**: Test strategy, Rust + OCaml testing, running tests
- **[CI_CD.md](docs/context/CI_CD.md)**: GitHub workflows, semantic versioning, release process
- **[GIT_HOOKS.md](docs/context/GIT_HOOKS.md)**: Pre-commit hooks, quality enforcement, troubleshooting
- **[TWO_BUFFER_DESIGN.md](docs/context/TWO_BUFFER_DESIGN.md)**: Technical design document for layout algorithms