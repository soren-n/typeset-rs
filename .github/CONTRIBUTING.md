# Contributing

## Setup

```bash
git clone https://github.com/soren-n/typeset-rs.git && cd typeset-rs
opam install dune qcheck typeset   # the OCaml reference the tests compare against
./scripts/install-hooks.sh         # the pre-commit gate
```

Rust stable (MSRV 1.96.0) and opam are the prerequisites.

## The rule

The compiler is a port of the OCaml `typeset` package and every change must
render byte-identically to it. The pre-commit hook and CI run the oracle
harness (`cd oracle && ./build.sh && ./_build/tester`); if it disagrees
with you, the reference is right. Exact-output tests take their expected
strings from the oracle, never from the Rust implementation.

## Pull requests

Keep commits focused and messages in conventional-commit style (`feat:`,
`fix:`, `refactor:`, `docs:`, `test:`, `chore:`; `!` for a breaking change).
The hook runs the same checks as CI, so a green hook is a green PR.
Breaking API changes go in `CHANGELOG.md` under the unreleased version.

Releases are tags cut by a maintainer; see
[docs/context/DEVELOPMENT.md](../docs/context/DEVELOPMENT.md).
