# Typeset Benchmarks

Two criterion suites live here. The full benchmarking and profiling guide,
including the audit history and what the numbers mean, is in
[docs/context/PERFORMANCE.md](../../docs/context/PERFORMANCE.md).

## Suites

### `layout_performance` — small-input latency

Groups: `construction`, `compilation`, `rendering`, `end_to_end`,
`reuse_efficiency`, `combinators`.

- `construction` / `compilation` / `rendering`: each phase on simple, nested,
  wide, and JSON-like layouts.
- `end_to_end`: construction through `render`, including `format_layout()`.
- `reuse_efficiency`: compile once and render at several widths versus
  recompiling each time.
- `combinators`: per-constructor cost (`text`, `comp`, `line`, `nest`, `pack`,
  `fix`, `grp`, `seq`).

### `scaling` — asymptotics

Groups: `compile_scaling`, `render_scaling`. Inputs are large enough to expose
growth (word chains to 64k, JSON-like trees to 32k leaves, nest-depth and
width sweeps). This is the suite to run before and after any pipeline or
renderer change.

## Running

```bash
cargo bench -p typeset                              # both suites
cargo bench -p typeset --bench scaling              # asymptotics only
cargo bench -p typeset --bench layout_performance -- rendering   # one group

# Compare a change against a saved baseline
cargo bench -p typeset --bench scaling -- --save-baseline before
# ... make the change ...
cargo bench -p typeset --bench scaling -- --baseline before
```

HTML reports are written to `target/criterion/` automatically (the
`html_reports` feature is enabled in the workspace).

## What to expect

- Every compile pass is linear in its input, and render is linear in output
  size and independent of the target width.
- Compile dominates render by roughly 15-25x at width 80, so the compile-once,
  render-many pattern in `reuse_efficiency` should win clearly.
- Compile is allocator-bound and performs a constant number of heap
  allocations regardless of document size; use `examples/alloc_probe.rs` to
  check that rather than inferring it from timing.

## Interpreting criterion output

Criterion's wall-clock percentage change on the compile benches is dominated
by code-layout noise: a refactor that shifts symbol addresses can report a
30-40% swing with no real change. For allocation-shape work trust
`alloc_probe` (exact counts) and an isolated, warmed `perf_probe` run over the
criterion delta. See PERFORMANCE.md for the cases where this bit.

## Adding a benchmark

Add a `fn bench_x(c: &mut Criterion)` that opens a `benchmark_group`, register
it in the suite's `criterion_group!` list, and prefer inputs at several scales
so growth is visible. Guard asymptotic behaviour, not wall-clock thresholds.
