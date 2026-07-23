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
- Render is width-independent for documents without grp/seq (`wide`, `packs`
  flat across width 20 → 100k) and linear in output size.
- Peak native stack is constant everywhere (flat arenas); depth costs heap.

### Costs to know about

- **Compile is allocator-bound.** ~40-50% of compile samples landed in
  malloc/free/memmove/memset at audit time. Allocation counting (`alloc_probe`)
  attributed most of it to input-tree teardown: `mem::take` on a `Box<Layout>`
  child allocated a placeholder box per edge in both the iterative `Drop` and
  `flatten`, and each dismantled node's own drop grew a fresh worklist —
  ~2.5 extra alloc/free pairs per node. Fixed 2026-07 (move the `Layout` value
  out of the box instead; skip leaf children): compile got 19-28% faster and
  now performs ~0.5 heap allocs per node on plain documents (essentially just
  `rescope`'s per-text `String` copy) and ~2.2 on grp/seq/nest-heavy ones
  (per-node satellite `Vec`s in graphify/rebuild/denull plus uncapacitied
  growth). The node arenas themselves are already amortized by `Vec` growth
  (~170 reallocs per 128k-node compile) — the flat-arena design is not the
  source of the traffic.
- **Was dropping the bump arenas a mistake? No — measured.** The flat `Vec`
  arenas do the same amortization job for node storage that bumpalo did, with
  better locality and eager frees (bumpalo is grow-only, so peak memory would
  be higher). What the bump removal genuinely re-introduced is per-object
  malloc for the small per-node satellite collections on scope-heavy inputs
  (~1.7 allocs/node on `json`); the better-than-bump fix is flattening those
  into shared side arrays with ranges (candidate list below). `serialize`
  keeps its bump because its persistent accumulators share structure — the one
  place bump semantics are load-bearing.
- **Compile dominates render** by roughly 5-15x at width 80. A layout compiled
  once and rendered at several widths amortizes well; per-keystroke recompiles
  pay the full pipeline each time.
- **Nest/pack wrapper distribution is O(leaves × wrapper depth).**
  `serialize` materializes the accumulated nest/pack path at every leaf,
  `denull` strips it to a prop list, `rescope` re-factors shared prefixes.
  `nestwide` confirms compile grows linearly in depth × leaves. Fine for
  code-shaped depth (~10-100); quadratic if depth grows with document size.
- **Render look-ahead is width-bounded, so grp/seq-heavy documents cost more
  at large widths.** Each `Seq` runs a full `will_fit` measure and nested seqs
  re-measure their subtrees; cost per decision is O(min(width, subtree
  extent)). The JSON workload renders ~5x slower at width 100k than at 20.
  Using a huge width to "disable wrapping" buys worst-case look-ahead.
- **Peak memory is ~0.5-1 KB per input node** (e.g. 188 MiB for a 512k-node
  chain; 899 MiB for a 1.3M-node JSON tree). Every intermediate representation
  stays alive until `compile` returns, because later IRs borrow text and terms
  from earlier ones; the peak is the sum of all of them plus the `Doc`.

### Optimization candidates, best first

Done (2026-07): renderer fold scratch reuse (24-48% render win) and
zero-allocation layout teardown in `Drop`/`flatten` (19-28% compile win; tree
drop went from 2.5 allocs/node to one allocation total).

1. **Precompute flat extents for `will_fit`.** A subtree without nest/pack is
   state-independent when measured flat; a one-pass bottom-up extent table in
   the `Doc` would make most `Seq` fit checks O(1) instead of O(width),
   removing the width sensitivity above (the Oppen/prettyplease approach).
2. **Flatten per-node satellite collections and cut per-pass churn.** Store
   graphify's `ins`/`outs`, denull's props, and rebuild's continuation lists
   as ranges into shared side arrays (CSR-style) instead of one `Vec` per
   node; `with_capacity` or reuse the vectors that grow (the `json` workload
   does ~0.65 reallocs per node); consider fusing the three normalize folds'
   rebuilds. This recovers the per-object amortization the old bump arenas
   provided, with better locality.
3. **Stop copying text in `rescope`.** Move the `String`s out of the
   `LayoutArena` (they are owned there and dead afterwards) or store one
   shared text buffer plus spans in `Doc`. Saves one malloc + copy per text
   node (the remaining ~0.5 allocs/node on plain documents) and shrinks `Doc`.
4. **Replace the renderer's pack-marks `HashMap` with a dense `Vec`** (pack
   indices are dense DFS counters; SipHash per lookup is waste). Only matters
   for pack-heavy layouts.

Ideas 1-4 are unimplemented; benchmark with `scaling` baselines before/after.
