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
- Compiling a plain (no grp/seq/nest) document performs a constant number of
  heap allocations (~57 for a 128k-node chain) — the arenas amortize
  everything; scope-heavy documents add ~0.33 allocs/node (was ~1.6 before
  the satellite/term flattening landed; the remainder is now graphify's
  BTreeMap of open scopes, the last per-object allocator on the compile path).

### Costs to know about

- **Compile is allocator-bound.** ~40-50% of compile samples landed in
  malloc/free/memmove/memset at audit time. Allocation counting (`alloc_probe`)
  attributed most of it to input-tree teardown: `mem::take` on a `Box<Layout>`
  child allocated a placeholder box per edge in both the iterative `Drop` and
  `flatten`, and each dismantled node's own drop grew a fresh worklist —
  ~2.5 extra alloc/free pairs per node. Fixed 2026-07 (move the `Layout` value
  out of the box instead; skip leaf children): compile got 19-28% faster.
  With the text-span, satellite-flattening, and flat-term reworks also
  landed, plain documents now compile with a constant number of heap
  allocations and grp/seq/nest-heavy ones with ~0.33 per node (now just
  graphify's BTreeMap of open scopes; serialize's scope accumulator became a
  flat arena and its bump is gone). The node arenas themselves are amortized
  by `Vec` growth — the flat-arena design was never the source of the traffic.
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
  unchanged and peak memory marginally lower.
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
  borrow only the layout arena's text now (scope deltas are ranges into an
  owned buffer, not bump slices), so `compile` drops each intermediate as
  soon as its consumer pass ran; the peak is a narrow window around the
  largest adjacent-IR pair plus the arena and the `Doc`. 512k-word chain:
  411 MB → 297 MB peak RSS. The residual ~0.5 KB/node peak is dominated by
  the layout arena (one `String` per text node) plus the `Doc`.

### Landed optimizations (2026-07)

Each verified byte-identical against the OCaml oracle; cumulative effect vs
the audit baseline: render 3-11x faster by workload; compile 25-32% faster on
plain word chains and now much more on scope- and nest-heavy documents (json
trees ~40% faster, deeply-nested `nestwide` up to ~8x); plain-document compile
down to a constant number of heap allocations, scope-heavy to ~0.33/node; peak
memory cut 28-90% by workload shape.

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

### Remaining candidates

1. **Move text out of the layout arena early.** The `LayoutArena` (one
   `String` per text node) now outlives every other intermediate only
   because text is borrowed from it; concatenating text into one buffer at
   `flatten` (spans instead of `&str`) would let the node arena drop after
   `resolve_breaks` and remove the last long-lived structure. It is the
   dominant term in the residual ~0.5 KB/node peak.
2. **Selective pass fusion.** The two normalize elimination folds have nearly
   identical shapes; fusing them saves one full arena rebuild. Fuse further
   only with care — the pass-per-file structure is a deliberate legibility
   choice.
3. **Arena-native construction** (builder API or macro-emitted arenas) to
   skip the `Box` tree entirely; public-API surface, only worth it if
   compile-per-keystroke latency becomes a use case.
4. **CI regression gating** — run the `scaling` bench (or instruction counts
   via iai-callgrind/CodSpeed on a Linux runner) automatically.

Benchmark any of these with `scaling` baselines before/after.
