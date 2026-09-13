# Development

## Building and testing

```bash
cargo build                       # every workspace member
cargo test --all                  # unit, integration, doc and macro tests
cargo run --example json_formatter -p typeset
```

### The oracle harness

The compiler is a port of the OCaml `typeset` package, and every change is
held to byte-identical output against it. `oracle/` holds the harness:

```bash
opam install dune qcheck typeset  # once
cd oracle && ./build.sh           # OCaml tester and oracle, Rust driver, into _build/
./_build/tester                   # 3000 generated layouts, both implementations
./compare.sh '"a" + grp ("b" + "c")' 2 3    # one expression, both implementations
./_build/oracle '"a" + grp ("b" + "c")' 2 3 # the reference alone
```

The tester is a QCheck property: for a generated layout, tab and width, the
reference's rendering equals the Rust driver's. The generator is biased
toward stacked grp/seq wrappers and narrow widths, which is where breaking
decisions diverge (a uniform generator missed a `grp(seq(x))` ordering bug
for fifteen runs). QCheck shrinks a failing case; the DSL it prints feeds
`compare.sh` directly.

The Rust tests in `typeset/tests/rendering.rs` pin exact outputs. Every
expected string there came from the oracle for the same layout, tab and
width; add cases the same way, never by pasting what the Rust
implementation printed.

### Code standards

- Never recurse on the native stack over user-controlled depth. Every
  representation is a flat postorder arena, so bottom-up folds are forward
  loops, inherited context is a backward loop, and `Clone`/`Drop`/`Debug`
  derive. Where a walk needs a stack (the DFS in `serialize`, the renderer,
  the DSL parser), it is an explicit `Vec` of frames.
- Use the arena primitives in `arena.rs`: `Arena<T>` with typed `Id<T>`s,
  `IdVec<K, V>` side tables, `Range<T>` into shared buffers, and
  `Option<Id<T>>` for absent links; never raw indices or sentinels.
- A pass owns the type it produces.
- Every compiler change is gated on the oracle harness. If the reference
  and the Rust implementation disagree, the reference is right.

## The pre-commit hook

Hooks are tracked in `.githooks/` and activated once per clone:

```bash
./scripts/install-hooks.sh        # git config core.hooksPath .githooks
```

The hook runs what CI runs: `cargo fmt --check`, `cargo clippy` with
warnings denied, `cargo check`, `cargo doc`, `cargo test`, then the oracle
harness. It fails if the OCaml toolchain is missing; `SKIP_OCAML=1` skips
the harness explicitly. The checks read the working tree, not the index, so
a partially staged commit is validated against everything on disk; CI
validates each pushed commit. `git commit --no-verify` bypasses the hook;
CI does not.

`cargo fmt` and `cargo clippy --fix --allow-dirty` apply the mechanical
fixes.

## CI

`.github/workflows/ci.yml` runs on every push and pull request:
- `check`: fmt, clippy, `cargo check`, `cargo doc` and `cargo test` on
  stable, and `cargo check` plus `cargo test` on the MSRV (1.96.0); lints
  run on stable alone so new lints never break the MSRV job. The committed
  `Cargo.lock` keeps the MSRV job deterministic.
- `deny`: `cargo deny` (advisories, the license allow-list in `deny.toml`,
  duplicate versions, sources).
- `differential`: installs OCaml, builds the oracle harness and runs the
  tester three times with three random seeds. A contributor without OCaml
  still gets their change checked against the reference here.

`dependencies.yml` runs `cargo audit` weekly. Dependabot opens grouped
weekly PRs for GitHub Actions and Cargo dependencies, and
`dependabot-auto-merge.yml` squash-merges them once CI passes (the repo has
no branch protection, so the gate lives in the workflow).

## Releasing

Both crates share one version from `[workspace.package]`. Merging to main
publishes nothing; a release is a tag.

```bash
./scripts/update-version.sh 5.0.0   # Cargo.toml and the lockfile
# add the version's section to CHANGELOG.md
git commit -am "chore(release): 5.0.0"
git tag v5.0.0 && git push origin main --tags
```

`release.yml` verifies the tag matches the workspace version, tests in
release mode, runs `cargo publish --workspace` (which publishes `typeset`
and then `typeset-parser`, waiting for each to be available, and skips the
unpublishable oracle driver), and creates the GitHub release pointing at
the changelog. A crates.io publish is immutable: a bad release is yanked
and a new version tagged.

## Benchmarking and profiling

```bash
cargo bench -p typeset --bench scaling                 # asymptotics
cargo bench -p typeset --bench scaling -- --save-baseline before
cargo bench -p typeset --bench scaling -- --baseline before
cargo bench -p typeset --bench perf_probe -- json 8 d=5 iters=5
cargo bench -p typeset --bench alloc_probe -- json 8 d=5
```

`scaling` is the criterion suite: compile and render at sizes large enough
to expose growth (word chains to 64k, JSON-like trees to 32k leaves,
nest-depth and width sweeps). Run it with a saved baseline before and after
any pipeline or renderer change. Every compile pass is linear in its input,
render is linear in output size and independent of the target width, and
compiling any document performs a constant number of heap allocations.

`perf_probe` prints one CSV line per run
(`workload,n,d,width,build_ns,compile_ns,render_ns,output_bytes`) with
best-of-`iters` timings; `loop=1 phase=compile|render` runs one phase
forever for a sampling profiler. `alloc_probe` counts heap traffic per phase
through a counting global allocator. The workloads (`wide`, `fixed`,
`lines`, `nestwide`, `deepgrp`, `packs`, `json`) are shared with the bench
in `typeset/benches/workloads.rs`.

Two things to know when reading numbers:
- Criterion's wall-clock change on the compile benches is dominated by
  code-layout noise: a refactor that shifts symbol addresses can report a
  30-40% swing with no real change, and the mirror swing on the next
  commit. Trust `alloc_probe` (exact counts) and a warmed `perf_probe` run
  over a criterion percentage.
- `perf_probe`'s first iteration is cold; compare the best of several.

On macOS the built-in sampler works without installing anything (set
`profile.bench.debug = true` so frames symbolicate; `cargo bench --bench
perf_probe --no-run` prints the binary path):

```bash
target/release/deps/perf_probe-* json 8 d=5 loop=1 phase=compile & PID=$!
sample $PID 5 1 -file profile.txt; kill $PID
```

`samply record <binary> <args>` gives the Firefox Profiler UI. Peak memory
is `/usr/bin/time -l <binary> <args>` (maximum resident set size).
Valgrind-based tools do not run on Apple Silicon.
