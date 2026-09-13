# Typeset Benchmarks

One criterion suite lives here. The full benchmarking and profiling guide,
including the audit history and what the numbers mean, is in
[docs/context/PERFORMANCE.md](../../docs/context/PERFORMANCE.md).

## The suite

`scaling` has two groups, `compile_scaling` and `render_scaling`, at inputs
large enough to expose growth (word chains to 64k, JSON-like trees to 32k
leaves, nest-depth and width sweeps). Run it before and after any pipeline or
renderer change.

## Running

```bash
cargo bench -p typeset --bench scaling
cargo bench -p typeset --bench scaling -- render    # one group

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
