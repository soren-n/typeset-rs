# Changelog

All notable changes are recorded here. This file is maintained by hand: add an
entry for each release before tagging it (see the release steps in
[CONTRIBUTING.md](.github/CONTRIBUTING.md)). Versions follow
[Semantic Versioning](https://semver.org/). Releases before 4.0.0 are in the
git history.

## [5.0.0] (unreleased)

### Breaking

* **One way to compile and render: `Layout::compile` and `Doc::render`.**
  The free functions `compile`, `render` and `format_layout` are gone.
* **The joins compose correctly.** `join_with_spaces` folds with `pad`
  (previously it inserted `" "` literals, so a broken sequence rendered
  whitespace-only lines) and `join_with_commas` fixes each comma to the item
  before it (previously a line could start with a comma). `join_with`,
  `space`, `comma`, `semicolon`, `newline`, `blank_line`, `parens`,
  `brackets` and `braces` are gone: each was one composition, and the
  delimiter helpers encoded a style choice. A blank line is
  `line(a, line(null(), b))`.
* **`Layout` is an opaque arena and nothing in the API is boxed.** Every
  constructor takes and returns `Layout` by value. `Layout`'s variants and
  the composition attribute type are no longer public; build layouts
  through the constructors. The joins take any `IntoIterator<Item = Layout>`.
  `Layout` no longer implements `Default`.
* **`typeset-parser` depends on `typeset`** (it feeds the shared parser) and
  reads its string literals with the DSL's own string syntax: the escapes
  `\n \r \t \0 \\ \" \'` are accepted, and raw strings, byte strings and
  other Rust escapes are compile errors. The crate depends on `proc-macro2`
  and `quote` alone.
* `typeset::dsl::ParseError` is a plain struct with public `at` (byte
  offset) and `message` fields; the accessor methods are gone.
* MSRV raised from 1.89.0 to 1.96.0.

### Added

* `typeset::dsl`: the DSL grammar with one implementation. `parse` reads a
  string at run time (dependency-free, iterative, byte-offset errors); the
  `layout!` macro feeds the same token parser, so the two front ends cannot
  disagree.
* Both crates' READMEs are their crate documentation, so every example in
  them is a doctest.

### Changed

* The compiler is two passes over flat arenas: `serialize` (lines of runs,
  scope deltas as stack pushes and pops; a sequence with a hard line
  beneath it is marked broken when it is built) and `structure` (the scope
  graph as a side table over the item buffer, solved in place, then read
  back straight into the `Doc` with two walks over a stack of open spines
  that drop empty terms, decide the grp/seq identities, right-nest every
  spine and factor shared nest/pack prefixes). Every item is a run of one
  or more fixed-joined terms; a `Doc` run is one contiguous string. The
  fix tree, the term leaf enum, the `Null` node, the rebuilt intermediate
  and the lowering pass are gone (`null()` is `text("")`, which the
  reference treats identically). Typed arena ids and `Option` links replace
  sentinels throughout.
* The renderer's head-of-line fit measure is a left-spine walk plus the
  precomputed extent; the measuring frame stack is gone.
* `Layout` clones and drops in constant allocations regardless of size and
  compile stays constant-allocation; `json 8 d=5` compiles about twice as
  fast as 4.1.0 and renders 15-20% faster on pack- and scope-heavy
  documents.
* Output is byte-identical to the OCaml reference on the QCheck identity
  suite, whose generator is biased toward stacked grp/seq and which now
  keeps one driver process for the run: 20000 cases in about a second. The
  harness lives in `oracle/`; the pre-commit hook no longer skips it
  silently. The profiling probes are harness-less bench targets sharing one
  workload module with the scaling bench.
* Releases publish with `cargo publish --workspace`; `Cargo.lock` is
  committed; the context docs are two files.

## [4.1.0](https://github.com/soren-n/typeset-rs/compare/v4.0.0...v4.1.0) (2026-07-23)

### Performance

* **`FixedDoc` is flat: no per-line or per-run allocation.** `split_lines`
  built `FixedDoc` as a `Vec` of lines, each owning a `Vec` of items and a
  `Vec` of separators, and each coalesced fixed run owning its own `Vec` of
  terms and separators — so a document with N fixed runs paid ~2N heap
  allocations (a scope- and comma-heavy `json` tree made ~half its leaves into
  length-1 runs). Items, line separators, run terms, and run separators now
  live in four shared arenas on `FixedDoc`, with each line and fix run holding
  `(start, end)` ranges into them; `split_lines` appends instead of allocating.
  This was the compile path's last per-node allocator: scope-heavy compilation
  drops from ~0.33 allocations per input node to a *constant* (`json 8 d=5`:
  65,596 to 64 compile allocations), the same constant-allocation regime plain
  documents already enjoyed, and compiles ~18% faster there. Byte-identical
  output (OCaml oracle plus 40k differential-fuzz rounds).
* **Layout text lives in one buffer and the node arena drops early.**
  `flatten` used to move each leaf's text into its own arena node (one heap
  `String` per text), so the whole layout node arena had to live to the end of
  compilation — every later representation borrowed its text from those nodes.
  It now concatenates all text into one buffer returned alongside the arena;
  node text becomes an 8-byte span into it. The buffer (small) is the only
  early structure that outlives the pipeline, and the node arena — now
  text-free — drops the moment `resolve_breaks` has read it. Peak memory falls
  8–15% on text- and structure-heavy documents (512k-word chain 277 to 237
  MiB, a 200k-node pack-heavy tree 548 to 475 MiB, `json` 66 to 61 MiB).
  Compile time and allocation counts are unchanged (the one added
  concatenation copy is negligible against the pipeline).
* **No bump arena remains.** `serialize`'s grp/seq scope accumulator was the
  last `bumpalo` user — a persistent pointer-linked list allocated in the
  pipeline's one bump. It is now a flat parent-linked arena (ids into a shared
  `Vec`, `depth` field and id equality replacing `ptr::eq`, exactly like the
  nest/pack path arena), so `serialize` owns every accumulator in a `Vec` it
  frees on return and the `bumpalo` dependency is dropped entirely. Compile
  work is unchanged (allocation counts identical); peak memory is marginally
  lower now that the bump's grow-only chunk headroom is gone (~1–6% by shape).
* **Term prop lists live in one shared buffer.** `denull` strips each term's
  nest/pack wrappers into a single document-wide prop buffer and terms carry
  `(start, end)` ranges into it, instead of one heap `Vec` per wrapped term.
  Every downstream prop operation (`rescope`'s prefix factoring included) is
  range arithmetic on the shared buffer, so `DenullTerm` is now `Copy` and
  `normalize`'s term moves are free. ~35% fewer compile allocations and ~12%
  faster compilation on nest-heavy documents (`json`: 1.57 to 1.01 allocs per
  input node).
* **The scope graph is intrusive lists over shared arrays.** The whole
  document now shares one node array and one edge pool; a node's incident
  edges are intrusive linked lists threaded through the pool instead of two
  heap `Vec`s per node, and graph nodes borrow their line's items rather than
  cloning fixed runs and pad lists. Solving the graph — pop a list head,
  insert before a known edge, splice one list into another — became O(1)
  pointer rewiring, which also removes the linear position scans the old
  vector surgery needed. Building the graph allocates nothing per node or
  edge (`json`: 1.01 to 0.55 compile allocs per input node, compile another
  ~13% faster).
* **The rebuild continuations are flat reused buffers.** Reading the solved
  scope graph back into a composition spine allocated a fresh `Vec` per
  opened scope (one continuation vector each, plus a partial-spine vector per
  close). The continuation stack is now one flat step vector delimited by a
  bounds stack, partial spines are ranges into one shared buffer, and all of
  it is reused across lines — threading continuations allocates nothing per
  scope (`json`: 0.55 to 0.42 compile allocs per input node; compile-phase
  reallocations drop from ~23k to ~170).
* **Terms are flat (path, leaf) values over a shared path arena.** `serialize`
  materialized every leaf's accumulated nest/pack wrappers as a bump-allocated
  chain — O(leaves × depth) memory. It now pushes one path-arena node per
  `Nest`/`Pack` it descends through, so sibling leaves share their wrapper
  path and a term is a copyable `(path id, leaf)` pair; `denull` materializes
  each distinct path into the prop buffer once (memoized) instead of once per
  term. Wrapper storage drops from O(leaves × depth) to O(input tree):
  compiling 1000 words under 1024 nests is ~8x faster and peak memory falls
  from 87 MB to 8 MB; `json` peak memory drops ~22% and compile ~10% more.
* **Intermediate representations drop mid-compile.** The serial output now
  owns its scope-delta buffer (ranges into one shared `Vec<Scope>` instead of
  bump-allocated slices), so nothing downstream references `serialize`'s bump
  and the borrow chain is broken: the bump and Edsl arena drop when
  `serialize` returns, the line structure after the scope graph is rebuilt,
  and the rebuilt document after denulling. Only the layout arena (the text
  owner) lives until the heap `Doc` is built — peak memory is the largest
  adjacent-IR window, not the sum of all IRs (512k-word chain: peak RSS 411
  to 297 MB; `json`: another ~8% off peak, compile ~12% faster).
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
