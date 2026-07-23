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
  everything; scope-heavy documents add ~1.6 allocs/node in satellite
  structures (see remaining candidates).

### Costs to know about

- **Compile is allocator-bound.** ~40-50% of compile samples landed in
  malloc/free/memmove/memset at audit time. Allocation counting (`alloc_probe`)
  attributed most of it to input-tree teardown: `mem::take` on a `Box<Layout>`
  child allocated a placeholder box per edge in both the iterative `Drop` and
  `flatten`, and each dismantled node's own drop grew a fresh worklist —
  ~2.5 extra alloc/free pairs per node. Fixed 2026-07 (move the `Layout` value
  out of the box instead; skip leaf children): compile got 19-28% faster.
  With the text-span rework also landed, plain documents now compile with a
  constant number of heap allocations and grp/seq/nest-heavy ones with ~1.6
  per node (the remaining satellite `Vec`s in graphify/rebuild/denull). The
  node arenas themselves are amortized by `Vec` growth — the flat-arena
  design was never the source of the traffic.
- **Was dropping the bump arenas a mistake? No — measured.** The flat `Vec`
  arenas do the same amortization job for node storage that bumpalo did, with
  better locality and eager frees (bumpalo is grow-only, so peak memory would
  be higher). What the bump removal genuinely re-introduced is per-object
  malloc for the small per-node satellite collections on scope-heavy inputs
  (~1.6 allocs/node on `json`); the better-than-bump fix is flattening those
  into shared side arrays with ranges (remaining candidate 1). `serialize`
  keeps its bump because its persistent accumulators share structure — the one
  place bump semantics are load-bearing.
- **Compile dominates render** by roughly 15-25x at width 80. A layout
  compiled once and rendered at several widths amortizes well; per-keystroke
  recompiles pay the full pipeline each time.
- **Nest/pack wrapper distribution is O(leaves × wrapper depth).**
  `serialize` materializes the accumulated nest/pack path at every leaf,
  `denull` strips it to a prop list, `rescope` re-factors shared prefixes.
  `nestwide` confirms compile grows linearly in depth × leaves. Fine for
  code-shaped depth (~10-100); quadratic if depth grows with document size.
- **Peak memory is ~0.5-1 KB per input node** (e.g. 188 MiB for a 512k-node
  chain; 899 MiB for a 1.3M-node JSON tree). Every intermediate representation
  stays alive until `compile` returns, because later IRs borrow text and terms
  from earlier ones; the peak is the sum of all of them plus the `Doc`.

### Landed optimizations (2026-07)

Each verified byte-identical against the OCaml oracle; cumulative effect vs
the audit baseline: render 3-11x faster by workload, compile 26-32% faster,
plain-document compile down to a constant number of heap allocations.

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

### Remaining candidates

1. **Flatten the remaining satellite collections.** graphify's `ins`/`outs`,
   denull's props lists, and rebuild's continuation vectors still cost ~1.6
   allocs/node on scope-heavy documents; CSR-style ranges into shared side
   arrays would finish the job (solve's in-place list surgery is the hard
   part).
2. **Flat term representation from `serialize` onward.** The bump-allocated
   `Term` wrapper chains are the last pointer-linked IR and the source of the
   O(leaves × depth) memory above; `(props-range, text)` terms would delete
   `strip_term` and likely retire the remaining `Bump`.
3. **Break the borrow chain to drop IRs early.** Text moving through the
   pipeline (rather than being borrowed from the `LayoutArena`) would let
   early IRs drop mid-compile; estimated 30-50% peak-memory cut.
4. **Selective pass fusion.** The two normalize elimination folds have nearly
   identical shapes; fusing them saves one full arena rebuild. Fuse further
   only with care — the pass-per-file structure is a deliberate legibility
   choice.
5. **Arena-native construction** (builder API or macro-emitted arenas) to
   skip the `Box` tree entirely; public-API surface, only worth it if
   compile-per-keystroke latency becomes a use case.
6. **CI regression gating** — run the `scaling` bench (or instruction counts
   via iai-callgrind/CodSpeed on a Linux runner) automatically.

Benchmark any of these with `scaling` baselines before/after.
