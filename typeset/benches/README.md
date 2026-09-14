# Benchmarks

`scaling` is the criterion suite: `compile_scaling` and `render_scaling` at
inputs large enough to expose growth (word chains to 64k, JSON-like trees to
32k leaves, nest-depth and width sweeps). `perf_probe` and `alloc_probe` are
harness-less profiling tools that share the workload generators in
`workloads/`. The full guide, including how to read the numbers, is in
[docs/context/DEVELOPMENT.md](../../docs/context/DEVELOPMENT.md).

```bash
cargo bench -p typeset --bench scaling
cargo bench -p typeset --bench scaling -- render                # one group
cargo bench -p typeset --bench scaling -- --save-baseline before
cargo bench -p typeset --bench scaling -- --baseline before     # after the change
cargo bench -p typeset --bench perf_probe -- json 8 d=5 iters=5
cargo bench -p typeset --bench alloc_probe -- json 8 d=5
```

Criterion writes HTML reports to `target/criterion/`. Its wall-clock
percentage on the compile benches is dominated by code-layout noise; trust
`alloc_probe` (exact counts) and a warmed `perf_probe` run over it.

To add a benchmark, add a `fn bench_x(c: &mut Criterion)` that opens a
`benchmark_group`, register it in `criterion_group!`, and use inputs at
several scales so growth is visible. Guard asymptotic behaviour, not
wall-clock thresholds.
