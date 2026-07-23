# Changelog

All notable changes are recorded here. This file is maintained by hand: add an
entry for each release before tagging it (see the release steps in
[CONTRIBUTING.md](.github/CONTRIBUTING.md)). Versions follow
[Semantic Versioning](https://semver.org/). Entries below `3.2.1` were generated
by the previous automated release tooling.

## [Unreleased]

### Performance

* **Line-break decisions are O(1).** `compile` now precomputes two per-object
  tables in the `Doc`: the flat mid-line extent (neither nest nor pack
  advances the position mid-line, so it is an exact sum) and the mid-line
  distance to the first composition boundary. `should_break` is pure
  arithmetic and `will_fit` only walks the document at the head of a line, so
  rendering no longer scans up to a line-width per decision. Render cost is
  now flat in the target width — grp/seq-heavy documents rendered at very
  large widths ("disable wrapping") were up to 7x slower before. Output is
  byte-identical; compile pays ~3% to build the tables once.
* **Pack marks are a dense vector and the output buffer is pre-sized.** Pack
  indices are dense DFS counters, so the renderer keys its marks by plain
  vector index (slot count stored in the `Doc`) instead of hashing into a
  `HashMap`; the output `String` reserves the document's text bytes up front.
  Another ~30% off pack-heavy rendering.
* **Document text lives in one shared buffer.** `Doc` text nodes hold 8-byte
  spans into a single concatenated `String` instead of one heap `String`
  each, and the renderer's per-row frame stack is reused across rows.
  Compiling a plain word chain now performs ~57 heap allocations total
  (previously one per text node), text nodes shrink to a quarter of their
  size, and rendering many-row documents is ~2x faster.
* **The compile passes pre-size their arenas and reuse scratch buffers.**
  `denull`, `rescope`, and the scope-graph rebuild reserve their output
  vectors from the known input sizes, and reassociation's chain
  materialization reuses one pair of scratch vectors instead of allocating
  per grp/seq boundary. ~8-10% off compilation of grp/seq-heavy documents.
* **The renderer's measuring folds reuse their work buffers.** Each line-break
  decision allocated two fresh `Vec`s (the fold's frame stack and its
  inserted-marks undo list); the renderer now owns one set of buffers and
  threads them through every fold. Render output is byte-identical and
  24-48% faster across the audit workloads (pack-heavy layouts gained most).
* **Layout teardown no longer allocates.** The iterative `Drop` and `flatten`
  dismantled the `Box<Layout>` tree with `mem::take` on each child box, which
  allocates a placeholder box per edge (`Box::default()`), and every
  dismantled node's own drop grew a fresh worklist — ~2.5 alloc/free pairs
  per node of pure overhead. Children now move out of their boxes by value
  (leaf children skipped), so dropping a tree performs a single allocation
  (the worklist) and compile is 19-28% faster across the audit workloads.

### Added

* **A scaling benchmark suite** (`cargo bench -p typeset --bench scaling`):
  compile and render at sizes that expose asymptotics, including a nest-depth
  sweep and a render width sweep, complementing the small-input
  `layout_performance` bench.
* **A profiling probe** (`typeset/examples/perf_probe.rs`): scalable workload
  generators with CSV timing output and a `loop=1` mode for attaching
  sampling profilers.
* **An allocation probe** (`typeset/examples/alloc_probe.rs`): per-phase heap
  traffic counts (allocs/frees/reallocs/bytes, per input node) via a counting
  global allocator.
* **[docs/context/PERFORMANCE.md](docs/context/PERFORMANCE.md)**: how to
  benchmark and profile the crate, plus the 2026-07 resource-usage audit's
  findings and ranked optimization candidates.

## [4.0.0](https://github.com/soren-n/typeset-rs/compare/v3.2.1...v4.0.0) (2026-07-22)

A **major version bump**: the entries below include breaking API changes.
Migrate per the notes in each item.

### Breaking Changes

* **`comp`'s composition axes are now enums.** `comp(left, right, pad: bool, fix:
  bool)` becomes `comp(left, right, pad: Pad, brk: Break)`, with `Pad::{Padded,
  Unpadded}` and `Break::{Breakable, Fixed}` exported from the crate root.
  Migration: on the pad axis `true` → `Padded`, `false` → `Unpadded`; on the
  break axis `true` → `Fixed`, `false` → `Breakable`. The
  `pad`/`unpad`/`fix_pad`/`fix_unpad` shortcuts are unchanged.
* **The depth-limiting and error-handling API is removed.** `compile_safe`,
  `compile_safe_with_depth`, `compile_within_depth`, `CompilerError`, and
  `DepthLimitExceeded` are all gone — the crate no longer exports an error type.
  `compile()` is the sole entry point and is infallible: the pipeline is fully
  iterative, so no layout is too deep to compile and there is no depth cap.
  Migration: replace `compile_safe(l)` / `compile_within_depth(l, n)` with
  `compile(l)`, which returns `Box<Doc>` directly rather than a `Result`.
* **`text()` now accepts `impl Into<String>`** (so it takes `&str` or `String`);
  the separate `text_str()` is removed. Migration: drop `text_str`, call `text`.
* **`Doc` is now an opaque struct** (a flat `Vec`-backed arena) instead of a
  public enum; the `DocObj` / `DocObjFix` payload types are removed. `Doc` was
  already opaque in use — no public constructors, payload types unexported — so
  only code that pattern-matched `Doc`'s variants is affected.
* **`render` borrows the document and `render_ref` is removed.** Rendering only
  reads the `Doc`, so `render(&doc, tab, width)` is the single entry point and
  the same document renders repeatedly without cloning. Migration:
  `render(doc, ...)` → `render(&doc, ...)`; `render_ref(&doc, ...)` →
  `render(&doc, ...)`.
* **The `Display` impls on `Layout` and `Doc` are removed**, along with the
  hand-maintained `Debug` formats that reproduced the historical recursive
  representations byte-for-byte. `Doc`'s `Debug` is now derived (it prints the
  flat row/arena form); `Layout`'s `Debug` keeps the compact derived-style
  format. Migration: use `{:?}` for diagnostics; render for output.
* **`Attr` carries `Pad`/`Break` enums** instead of `pad`/`fix` booleans
  (`Attr { pad: Pad, brk: Break }`). Only relevant to code constructing
  `Layout::Comp` nodes directly rather than via `comp()`.

### Performance Improvements

* Deeply nested `grp`/`seq` scopes now compile in **linear time** (previously
  O(n²)); e.g. a 16k-deep nested-`seq` chain dropped from ~5s to ~9ms.
* The renderer's measuring folds are **width-bounded**: measurement stops as
  soon as the position passes the target width, so each break decision costs at
  most O(width) work instead of walking arbitrarily large subtrees.
* The whole pipeline now runs on flat postorder arenas: terms and text are
  borrowed (never copied) between passes, two passes fused
  (`linearize` + `fixed`), and all but one per-pass bump arena eliminated.

### Changed

* `compile()` no longer panics on very deep layouts (it previously aborted past
  ~10,000 levels). Every intermediate representation is a flat arena folded
  with plain loops, so deep layouts never overflow the native stack; depth
  shows up as O(depth) heap instead.
* The compiler passes were renamed for what they do (`broken` →
  `resolve_breaks`, `fixed` → `split_lines`, `structurize` → `resolve_scopes`)
  and the scope-graph solver was rewritten from intrusive `Cell`-linked lists
  to indexed adjacency. Internal only — pass modules are not part of the public
  API.
* Legibility pass over both crates (internal only, output byte-identical): the
  per-pass arena appenders were unified into one generic `push_node` helper;
  the iterative tree walks (`flatten`, `Layout`'s `Clone`/`Debug`) carry unary
  node constructors as function pointers instead of re-matching task variants;
  `split_lines` accumulates lines through a small builder struct; and the
  parser's alternative-combinator and DSL reification collapsed to plain
  early-return loops and per-operator `reify` methods.

## [3.2.1](https://github.com/soren-n/typeset-rs/compare/v3.2.0...v3.2.1) (2026-07-21)


### Bug Fixes

* make Doc/DocObj/DocObjFix/Layout Debug iterative ([a4c3fe2](https://github.com/soren-n/typeset-rs/commit/a4c3fe20e2330508897f12a88036d15e3bb52d84))

# [3.2.0](https://github.com/soren-n/typeset-rs/compare/v3.1.7...v3.2.0) (2026-07-21)


### Bug Fixes

* make Doc clone iterative to keep deep documents from overflowing ([c7e7835](https://github.com/soren-n/typeset-rs/commit/c7e78350061cb0759bca1cb1dd2d98c97fe0a3b6))
* make the Layout AST deep-safe (iterative Drop, Clone, Display) ([399847f](https://github.com/soren-n/typeset-rs/commit/399847f0850b200ee33605faea9549c978854d74))


### Features

* add render_ref for rendering a document by reference ([128a986](https://github.com/soren-n/typeset-rs/commit/128a98629a3135f7438e3858217286ae7e845b28))


### Performance Improvements

* defunctionalize move_to_heap to remove native-stack recursion ([d67c470](https://github.com/soren-n/typeset-rs/commit/d67c470e9820a0a512752d2239508c1888cf5af3))
* defunctionalize the renderer to remove native-stack recursion ([102c32a](https://github.com/soren-n/typeset-rs/commit/102c32a524a7ae1102d51d550350f7653b0ac3b8))
* make Doc drop and Display iterative ([9292e1c](https://github.com/soren-n/typeset-rs/commit/9292e1cb146c830c05a969e16e1e006d7eadce8b))

## [3.1.7](https://github.com/soren-n/typeset-rs/compare/v3.1.6...v3.1.7) (2026-07-21)


### Bug Fixes

* make List::get and get_unsafe iterative ([afae985](https://github.com/soren-n/typeset-rs/commit/afae98588dc68506f42f5f59ebb917e309fe75e7))


### Performance Improvements

* defunctionalize the broken pass to remove native-stack recursion ([4d76641](https://github.com/soren-n/typeset-rs/commit/4d766416a21699fa30e989ff1dfa8d94f0f98245))
* defunctionalize the denull pass to remove native-stack recursion ([3ddda36](https://github.com/soren-n/typeset-rs/commit/3ddda3685ab255d7199836b42fb55a93f848037f))
* defunctionalize the fixed pass to remove native-stack recursion ([db9704c](https://github.com/soren-n/typeset-rs/commit/db9704cf5510b466513de0ef5904e15d298bdabf))
* defunctionalize the identities pass to remove native-stack recursion ([0235d46](https://github.com/soren-n/typeset-rs/commit/0235d46ffc83cb94dab60a1c818a3fa4018f0f4b))
* defunctionalize the linearize pass to remove native-stack recursion ([b5c1cc5](https://github.com/soren-n/typeset-rs/commit/b5c1cc51d6d05b390e3f1bcc371535a862346674))
* defunctionalize the reassociate pass to remove native-stack recursion ([b2cd7ea](https://github.com/soren-n/typeset-rs/commit/b2cd7ea84c9f8631092720f1976b7c45dba5c1dc))
* defunctionalize the rescope pass to remove native-stack recursion ([178200a](https://github.com/soren-n/typeset-rs/commit/178200a7f00ba8ca7cd1946bfefc020e5694723c))
* defunctionalize the serialize pass to remove native-stack recursion ([70b81ed](https://github.com/soren-n/typeset-rs/commit/70b81ed3ed0ea831fa8df4cc99cf3e261660e350))
* defunctionalize the structurize pass to remove native-stack recursion ([3f2ac45](https://github.com/soren-n/typeset-rs/commit/3f2ac45b130c92d973a95c8a8c862eb51747e600))

## [3.1.6](https://github.com/soren-n/typeset-rs/compare/v3.1.5...v3.1.6) (2026-07-20)


### Bug Fixes

* correct avl remove and get_member; add proptest coverage ([024b1a0](https://github.com/soren-n/typeset-rs/commit/024b1a0ece852fe0d7777717f231e3d0e4bde12a))
* produce in-order output from avl::to_list ([ffd634e](https://github.com/soren-n/typeset-rs/commit/ffd634e33be25a6d88b539785ddf4e2344f258c0))


### Performance Improvements

* drop redundant identity fold over Map::values in structurize ([653941d](https://github.com/soren-n/typeset-rs/commit/653941d6ce989c5d248d77ae20e9a03016a192dd))

## [3.1.5](https://github.com/soren-n/typeset-rs/compare/v3.1.4...v3.1.5) (2026-07-20)


### Bug Fixes

* enforce the max_depth limit in compile_safe_with_depth ([5f30562](https://github.com/soren-n/typeset-rs/commit/5f305622d6e6a998f65e031c14d5f228ad400eb7))
* measure text width in characters, not UTF-8 bytes ([df541a1](https://github.com/soren-n/typeset-rs/commit/df541a104eab80015895a04366bf25c1dd738eab))
* **tests:** parse the @@ operator in the unit test grammar ([1505de2](https://github.com/soren-n/typeset-rs/commit/1505de24eb8e4e96f5dcc4801fb167b589c2c863))
* **tests:** propagate exit code and reap children in OCaml tester ([5ad7573](https://github.com/soren-n/typeset-rs/commit/5ad75734880fc3ece9c1f3363e9267f678d01c5a))
* use the post-insert subtree when updating AVL height ([832c538](https://github.com/soren-n/typeset-rs/commit/832c538b69562b911f703ebc5c44c64108f67832))


### Performance Improvements

* drop redundant deep clones in the broken pass ([84fb26c](https://github.com/soren-n/typeset-rs/commit/84fb26c0493e85837da9ef3600c32e888a7eeef6))

## [3.1.4](https://github.com/soren-n/typeset-rs/compare/v3.1.3...v3.1.4) (2026-05-18)


### Bug Fixes

* **ci:** allow Unicode-3.0 + first-party GPL crates in cargo-deny ([1eb5d20](https://github.com/soren-n/typeset-rs/commit/1eb5d2053ab1fff8a580d036d15e39f1bc3d03c8))

## [3.1.3](https://github.com/soren-n/typeset-rs/compare/v3.1.2...v3.1.3) (2026-05-18)


### Bug Fixes

* **ci:** migrate deny.toml to cargo-deny v2 schema ([d3c6080](https://github.com/soren-n/typeset-rs/commit/d3c6080c7e7b7ab4f0bca2c7224bc5d84cf5f386))

## [3.1.2](https://github.com/soren-n/typeset-rs/compare/v3.1.1...v3.1.2) (2026-02-17)


### Bug Fixes

* correct version mismatch and remove dead code ([a20b197](https://github.com/soren-n/typeset-rs/commit/a20b1976bc70d0fb6830685ff4009d172fd82cdc))

## [3.1.1](https://github.com/soren-n/typeset-rs/compare/v3.1.0...v3.1.1) (2026-02-17)


### Bug Fixes

* **tests:** replace deprecated QCheck.Gen APIs in OCaml tester ([9ce6c81](https://github.com/soren-n/typeset-rs/commit/9ce6c81966141e3c34a5a82dd0e78242e0573858))

# [3.1.0](https://github.com/soren-n/typeset-rs/compare/v3.0.5...v3.1.0) (2025-08-17)


### Features

* add stable Rust support with MSRV 1.89.0 ([746ecdd](https://github.com/soren-n/typeset-rs/commit/746ecdd23678c03223491aa947df5c553d538bfc))

## [3.0.5](https://github.com/soren-n/typeset-rs/compare/v3.0.4...v3.0.5) (2025-08-17)


### Bug Fixes

* **ci:** add --allow-dirty flag for publishing modified Cargo.toml ([13aba9c](https://github.com/soren-n/typeset-rs/commit/13aba9cde191fa4edc99006b3ea0eb65760fb67d))

## [3.0.4](https://github.com/soren-n/typeset-rs/compare/v3.0.3...v3.0.4) (2025-08-17)


### Bug Fixes

* **ci:** resolve circular dependency during crate publishing ([ee10693](https://github.com/soren-n/typeset-rs/commit/ee1069383b5347edd90b16de9f923ba9ee05eb05))

## [3.0.3](https://github.com/soren-n/typeset-rs/compare/v3.0.2...v3.0.3) (2025-08-17)


### Bug Fixes

* **ci:** publish typeset-parser before typeset to resolve dependency issue ([4eaecd7](https://github.com/soren-n/typeset-rs/commit/4eaecd7202807cf5217de9ab59d94c5a4cc572b9))

## [3.0.2](https://github.com/soren-n/typeset-rs/compare/v3.0.1...v3.0.2) (2025-08-17)


### Bug Fixes

* **ci:** set GitHub Actions outputs for semantic-release job ([0bd32bd](https://github.com/soren-n/typeset-rs/commit/0bd32bdaccf62bf22798f7c29d8f5258d90506b0))

## [3.0.1](https://github.com/soren-n/typeset-rs/compare/v3.0.0...v3.0.1) (2025-08-17)


### Bug Fixes

* **release:** update version script to handle bidirectional dependencies ([7eeabca](https://github.com/soren-n/typeset-rs/commit/7eeabca81eb6456517579478c30a5f6fa9a201b6))

# [3.0.0](https://github.com/soren-n/typeset-rs/compare/v2.0.5...v3.0.0) (2025-08-17)


### Bug Fixes

* add missing CI workflow file ([6c95482](https://github.com/soren-n/typeset-rs/commit/6c95482338306ef7a556d56acb8a8f46e70ae004))
* **ci:** resolve GitHub Actions workflow failures ([942fb7c](https://github.com/soren-n/typeset-rs/commit/942fb7c73266f6c36a3f990e7c912fcc2245b50a))
* **ci:** resolve remaining workflow issues ([27a524b](https://github.com/soren-n/typeset-rs/commit/27a524bb1250a74b333bd4e1c5cd8b322ef52e44))
* **ci:** temporarily disable OCaml and security audit jobs ([afb916f](https://github.com/soren-n/typeset-rs/commit/afb916fc842ef24a4b233205125aec45c32b56c1))
* **release:** resolve semantic-release sed command syntax error ([264f080](https://github.com/soren-n/typeset-rs/commit/264f080f1c2831d580e60e6ec035d46ddb4d7952))
* resolve CI/CD workflow failures ([645022f](https://github.com/soren-n/typeset-rs/commit/645022f73f61d6e06b71ac1f21f50871a37b1b17))


### Features

* add comprehensive git pre-commit hooks ([b0e6047](https://github.com/soren-n/typeset-rs/commit/b0e6047c869ae24db2dd17265af2b208d1aaf773))
* implement comprehensive CI/CD with semantic versioning ([a729fc7](https://github.com/soren-n/typeset-rs/commit/a729fc7855f661be72069ef26ec0dd799a29fbaa))
* improve OCaml testing support in git hooks ([37e9076](https://github.com/soren-n/typeset-rs/commit/37e9076b6d0476c04252e000165a751a51686407))
* major restructure and improvements ([7ee88ea](https://github.com/soren-n/typeset-rs/commit/7ee88eac42a46b7cef9897c8364c003cf2990edc))


### BREAKING CHANGES

* CI/CD pipeline now requires conventional commit messages for releases

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive CI/CD pipeline with GitHub Actions
- Automatic semantic versioning based on conventional commits
- OCaml integration testing in CI
- Security vulnerability scanning
- Automated dependency updates
- Pre-commit git hooks for code quality
- Comprehensive documentation for contributors

### Changed
- Modernized GitHub Actions workflows
- Enhanced code quality gates
- Improved development workflow

### Fixed
- Updated deprecated GitHub Actions
- Resolved clippy warnings and formatting issues

---

*Note: This changelog is automatically maintained by semantic-release based on conventional commit messages.*
