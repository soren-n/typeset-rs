# Performance

How to benchmark and profile the compiler and renderer, and what the 2026-07
resource-usage audit found. Numbers below are from an Apple Silicon Mac
(Darwin 25), release builds; treat them as shapes, not absolutes.

## Tools

### Benchmarks (in-repo)

- `cargo bench -p typeset --bench scaling` — compile and render at sizes large
  enough to expose asymptotics (1k-64k words, JSON-like trees to 32k leaves,
  nest-depth and width sweeps). Run this before/after any pipeline or renderer
  change; use criterion baselines to diff:
  `cargo bench -p typeset --bench scaling -- --save-baseline before` then
  `-- --baseline before` after the change.
- `cargo bench -p typeset --bench layout_performance` — small-input latency and
  per-combinator costs.

### Profiling probe

`typeset/examples/perf_probe.rs` generates scalable workloads and prints CSV
(`workload,n,d,width,build_ns,compile_ns,render_ns,output_bytes`):

```bash
cargo build --release --example perf_probe \
  --config 'profile.release.debug=true' --config 'profile.release.strip=false'
target/release/examples/perf_probe json 8 d=5 iters=5
```

Workloads: `wide N` (breakable word chain), `fixed N` (one fix run), `lines N`
(hard-line spine), `nestwide M d=D` (nest^D over M words), `deepgrp D`
(grp/nest nesting), `packs N` (pack-heavy), `json FAN d=D` (grp/seq/nest tree,
FAN^D leaves). `loop=1 phase=compile|render` runs one phase forever so a
sampling profiler can attach.

`typeset/examples/alloc_probe.rs` counts heap traffic (allocs/frees/reallocs/
bytes, absolute and per input node) for build, clone, drop, compile, and
render via a counting global allocator — use it to attribute allocator-bound
profiles before reaching for dhat:

```bash
cargo run --release --example alloc_probe -- json 8 d=5
```

### CPU profiling (macOS)

The built-in sampler works without any install (build with the debug/strip
flags above so frames symbolicate; run `dsymutil` on the binary if needed):

```bash
target/release/examples/perf_probe json 8 d=5 loop=1 phase=compile & PID=$!
sample $PID 5 1 -file profile.txt; kill $PID
```

For interactive work, `samply` (`cargo install samply`, then
`samply record <binary> <args>`) gives the Firefox Profiler UI with inline
Rust symbolication and is the current community default. `cargo-flamegraph`
needs root on macOS (dtrace); Instruments (`cargo instruments -t time`) is the
GUI alternative. Valgrind-based tools (iai-callgrind) do not work on Apple
Silicon — put instruction-count regression gating in Linux CI if wanted.

### Memory profiling (macOS)

- Peak RSS: `/usr/bin/time -l <binary> <args>` (bytes, `maximum resident set
  size`).
- Allocation sites: add `dhat` behind a feature and view `dhat-heap.json` in
  the online DHAT viewer, or `cargo instruments -t Allocations`. Divan's
  `AllocProfiler` gives per-bench alloc counts if benches migrate to divan.

## Audit findings (2026-07)

### Shapes confirmed good

- Every compile pass is linear in its input; compile time scales linearly for
  word chains, hard-line spines, fix runs, JSON-like trees, and grp/seq
  nesting (the old graphify O(n^2) stays fixed).
- Render is width-independent (since the extent tables, for all documents)
  and linear in output size.
- Peak native stack is constant everywhere (flat arenas); depth costs heap.
- Compiling *any* document — plain or scope-heavy — now performs a constant
  number of heap allocations (~57 for a 128k-node chain; ~64 for `json 8 d=5`);
  the arenas amortize everything. This was not always true: scope-heavy
  documents used to add ~0.33 allocs/node. Per-pass attribution (2026-07-23)
  placed ~98% of that in `split_lines`, which built `FixedDoc` with an owned
  `Vec` per line and an owned `Vec<Term>` + `Vec<FixedComp>` per coalesced
  fixed run (~2N allocations for N fixed runs). Flattening `FixedDoc` into
  shared arenas (landed optimization 12) eliminated it. (`graphify`'s
  open-scope `BTreeMap` — once believed to be this remainder — was never the
  source: json is a single hard-line-free line, so the map is created once and
  reused to a depth-bounded peak; resolve_scopes was ~13 of the 65,596
  allocations.)

### Costs to know about

- **Compile is allocator-bound.** ~40-50% of compile samples landed in
  malloc/free/memmove/memset at audit time. Allocation counting (`alloc_probe`)
  attributed most of it to input-tree teardown: `mem::take` on a `Box<Layout>`
  child allocated a placeholder box per edge in both the iterative `Drop` and
  `flatten`, and each dismantled node's own drop grew a fresh worklist —
  ~2.5 extra alloc/free pairs per node. Fixed 2026-07 (move the `Layout` value
  out of the box instead; skip leaf children): compile got 19-28% faster.
  With the text-span, satellite-flattening, flat-term, and `FixedDoc`-arena
  reworks also landed, *every* document — plain or grp/seq/nest-heavy — now
  compiles with a constant number of heap allocations (`json 8 d=5`: ~64). The
  last per-node allocator was `split_lines`'s per-line and per-fixed-run `Vec`s
  (landed optimization 12); `serialize`'s scope accumulator became a flat arena
  and its bump is gone. The node arenas themselves are amortized by `Vec`
  growth — the flat-arena design was never the source of the traffic.
- **Was dropping the bump arenas a mistake? No — measured.** The flat `Vec`
  arenas do the same amortization job for node storage that bumpalo did, with
  better locality and eager frees (bumpalo is grow-only, so peak memory would
  be higher). What the bump removal genuinely re-introduced is per-object
  malloc for the small per-node satellite collections on scope-heavy inputs
  (~1.6 allocs/node on `json` at audit time); the better-than-bump fix —
  flattening those into shared side arrays with ranges and intrusive lists —
  landed 2026-07 (see landed optimization 7). `serialize`'s persistent scope
  accumulator, the last bump user, later became a flat parent-linked arena
  (landed optimization 10) — the bump is gone entirely, with compile work
  unchanged and peak memory marginally lower. The ~0.33 allocs/node that used
  to remain on scope-heavy inputs was `split_lines`'s per-line and per-fixed-
  run `Vec`s, *not* `graphify` — a per-pass alloc probe put ~98% of it in
  `split_lines` and only ~13 allocations in all of resolve_scopes. Flattening
  `FixedDoc` into shared arenas (landed optimization 12) removed it, so
  scope-heavy compilation is now constant-allocation like plain documents.
- **Compile dominates render** by roughly 15-25x at width 80. A layout
  compiled once and rendered at several widths amortizes well; per-keystroke
  recompiles pay the full pipeline each time.
- **Nest/pack wrapper distribution was O(leaves × wrapper depth) — fixed
  2026-07.** `serialize` used to materialize the accumulated nest/pack path
  at every leaf; wrappers now live in a shared path arena (one node per
  `Nest`/`Pack` descended through, sibling leaves share their spine) and
  `denull` memoizes each distinct path's prop materialization, so wrapper
  storage is O(input tree). Compiling 1000 words under 1024 nests went from
  11 ms / 87 MB peak to 2 ms / 8 MB (see landed optimizations 8-9).
- **Peak memory was the sum of all live IRs — fixed 2026-07.** Later IRs
  borrow only the layout text now (scope deltas are ranges into an owned
  buffer, not bump slices; text is a span into one shared buffer), so
  `compile` drops each intermediate — the layout node arena included — as
  soon as its consumer pass ran; the peak is a narrow window around the
  largest adjacent-IR pair plus the text buffer and the `Doc`. 512k-word
  chain: 411 MB → 297 MB → 237 MB peak RSS across the two reworks. The
  residual peak is now dominated by the largest adjacent IR pair and the
  `Doc` rather than any single long-lived arena.

### Landed optimizations (2026-07)

Each verified byte-identical against the OCaml oracle; cumulative effect vs
the audit baseline: render 3-11x faster by workload; compile 25-32% faster on
plain word chains and now much more on scope- and nest-heavy documents (json
trees ~50% faster, deeply-nested `nestwide` up to ~8x); *every* document now
compiles with a constant number of heap allocations (scope-heavy included,
after the `FixedDoc`-arena rework); peak memory cut 28-90% by workload shape.

1. Renderer fold scratch reuse — the measuring folds allocated two `Vec`s per
   break decision (24-48% render).
2. Zero-allocation layout teardown — `mem::take` on `Box` children allocated
   a placeholder box per edge in `Drop`/`flatten` (19-28% compile; tree drop
   is now one allocation total).
3. Mid-line extent tables in the `Doc` — mid-line, nest/pack never advance
   the position, so flat extents and next-boundary distances are exact
   precomputable sums; `should_break` is pure arithmetic and `will_fit` only
   folds at the head of a line. Removed the O(width) look-ahead entirely
   (width-100k rendering 7x faster; render now flat in width).
4. Dense pack-mark vector (pack indices are dense DFS counters) and a
   pre-sized output `String` (~30% off pack-heavy rendering).
5. Pass arena pre-sizing and reassociation scratch reuse (~8-10% compile on
   grp/seq-heavy documents).
6. Text spans — `Doc` text nodes are 8-byte spans into one shared `String`;
   the renderer's per-row frame stack is reused (many-row rendering 2.25x
   faster; text nodes a quarter of their former size).
7. Satellite collection flattening (the former remaining candidate 1), in
   three steps: denull term props as ranges into one shared buffer
   (`DenullTerm` is `Copy`); the scope graph as intrusive linked edge lists
   over one shared node array and edge pool, borrowing the `FixedDoc` line
   items instead of cloning fix runs (solve's list surgery became O(1)
   pointer rewiring with no position scans); rebuild's continuation stack as
   one flat step vector plus a bounds stack with partial spines as ranges
   into a shared buffer, all reused across lines. Cumulative on `json 8 d=5`:
   compile allocs 1.57 → 0.42 per input node, compile ~22% faster.
8. Flat terms over a shared path arena — `serialize` no longer materializes
   per-leaf bump `Term` chains; a term is a `Copy` `(path id, leaf)` pair
   into a shared arena with one node per Nest/Pack, and `denull` memoizes
   path materialization. Killed the O(leaves × depth) tier: `nestwide 1000
   d=1024` compiles ~8x faster at 87 → 8 MB peak RSS.
9. Early IR drops — scope deltas became ranges into an owned `Vec<Scope>`
   (no bump references escape `serialize`), so each IR drops as soon as its
   consumer pass ran. 512k-word chain peak RSS 411 → 297 MB; json ~8% off
   peak and ~12% faster compile (0.33 allocs/node).
10. Last bump retired (the former remaining candidate 2) — `serialize`'s
    grp/seq scope accumulator, the only remaining `bumpalo` user, became a
    flat parent-linked arena (ids into a shared `Vec`, `depth` + id equality
    replacing `ptr::eq`, like the nest/pack path arena). The `bumpalo`
    dependency is dropped entirely; alloc counts are unchanged (the bump
    served its nodes from a few large chunks, the `Vec` from amortized
    growth) and peak memory is marginally lower now the bump's grow-only
    chunk headroom is gone (~1-6% by shape). Compile timing is unchanged —
    isolated warmed `perf_probe` showed it flat within run-to-run noise;
    criterion reported large (-30-44%) swings on the nest/scope-heavy
    compile benches, but those are the code-layout artifact (removing a
    dependency shifts every symbol address), not a real speedup.
11. Layout text moved out of the node arena (the former remaining candidate
    1) — `flatten` concatenates all leaf text into one buffer and text nodes
    hold an 8-byte span into it, instead of one owned `String` per text node.
    The buffer outlives the pipeline; the now-text-free node arena drops right
    after `resolve_breaks` instead of living to the end (every later IR
    borrows text from the buffer, not the arena). Peak RSS falls 8-15% on
    text/structure-heavy documents (512k-word chain 277 → 237 MiB, 200k-node
    pack tree 548 → 475 MiB, json 66 → 61 MiB; nestwide unchanged — its text
    is negligible next to the path arena). Compile timing and alloc counts
    unchanged (the one added concatenation copy is negligible). Criterion
    again showed large swings here (+9-43% on nest/scope-heavy compile
    benches) — the *mirror* of optimization 10's swings, and they cancel
    across the two commits; isolated warmed `perf_probe` distributions
    overlap, confirming flat. Trust `alloc_probe` (exact) and isolated
    `perf_probe` over criterion %-change on binary-layout-shifting refactors.
12. `FixedDoc` flattened into shared arenas (the former remaining candidate 1
    — the compile path's last per-node allocator). `split_lines` built
    `FixedDoc` with an owned `Vec` per line (items + separators) and an owned
    `Vec<Term>` + `Vec<FixedComp>` per coalesced fixed run, so a document with
    N fixed runs paid ~2N allocations — ~98% of the ~0.33 allocs/node on
    scope-heavy inputs (a per-pass alloc probe put 2,593 of `json 6 d=4`'s
    2,652 compile allocations there, and only 13 in all of resolve_scopes).
    Items, line separators, run terms, and run separators now live in four
    shared arenas on `FixedDoc`, with each line and fix run a `(start, end)`
    range (`FixedSpan`) into them; `split_lines` appends instead of allocating,
    `graphify`/`rebuild` resolve the ranges against the borrowed `FixedDoc`
    (node/item index-alignment preserved). Scope-heavy compilation drops to a
    *constant* allocation count (`json 8 d=5`: 65,596 → 64) — the regime plain
    documents already had — and runs ~18% faster there (`json 8 d=5` warmed
    compile ~20.9 → ~17.1 ms). Unlike the layout-shift noise in optimizations
    10–11, this speedup is backed by the exact alloc delta (65k fewer
    malloc/free pairs on an allocator-bound compile), not criterion. Byte-
    identical (OCaml oracle + 40k differential-fuzz rounds).
    Note: `graphify`'s open-scope `BTreeMap` was previously believed to be this
    remainder and was investigated first; it is not the source — json is a
    single hard-line-free line, so the map is created once and reused to a
    depth-bounded peak. A dense-`Vec` rewrite of it is byte-identical but saves
    ~0 allocations and slightly regresses peak memory, so it was not landed.

### Remaining candidates

1. **Selective pass fusion.** The two normalize elimination folds have nearly
   identical shapes; fusing them saves one full arena rebuild. Fuse further
   only with care — the pass-per-file structure is a deliberate legibility
   choice.
2. **Arena-native construction** (builder API or macro-emitted arenas) to
   skip the `Box` tree entirely; public-API surface, only worth it if
   compile-per-keystroke latency becomes a use case.
3. **CI regression gating** — run the `scaling` bench (or instruction counts
   via iai-callgrind/CodSpeed on a Linux runner) automatically. Especially
   worthwhile here: criterion's wall-clock %-change is dominated by
   code-layout noise on the compile benches, so gate on instruction counts
   (deterministic) rather than time.

Benchmark any of these with `scaling` baselines before/after.
