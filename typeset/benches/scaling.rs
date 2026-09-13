//! Scaling benchmarks: compile and render at sizes large enough to expose
//! asymptotics (the `layout_performance` bench covers small-input latency).
//!
//! The workloads are shared with `perf_probe` and `alloc_probe`, the profiling
//! companions to this bench.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use workloads::{json, nestwide, packs, wide};

mod workloads;

fn bench_compile_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile_scaling");
    group.sample_size(20);

    for n in [1_000, 8_000, 64_000] {
        let layout = wide(n);
        group.bench_with_input(BenchmarkId::new("wide", n), &n, |b, _| {
            b.iter(|| layout.clone().compile())
        });
    }
    for d in [3, 4, 5] {
        let layout = json(d, 8);
        let leaves = 8usize.pow(d as u32);
        group.bench_with_input(BenchmarkId::new("json_leaves", leaves), &d, |b, _| {
            b.iter(|| layout.clone().compile())
        });
    }
    for d in [64, 256, 1024] {
        let layout = nestwide(d, 1_000);
        group.bench_with_input(BenchmarkId::new("nestwide_1k_words", d), &d, |b, _| {
            b.iter(|| layout.clone().compile())
        });
    }

    group.finish();
}

fn bench_render_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_scaling");

    for n in [1_000, 8_000, 64_000] {
        let doc = wide(n).compile();
        group.bench_with_input(BenchmarkId::new("wide", n), &n, |b, _| {
            b.iter(|| doc.render(2, 80))
        });
    }
    for n in [1_000, 8_000, 64_000] {
        let doc = packs(n).compile();
        group.bench_with_input(BenchmarkId::new("packs", n), &n, |b, _| {
            b.iter(|| doc.render(2, 80))
        });
    }

    // Width sweep on a grp/seq-heavy document: the renderer's look-ahead is
    // width-bounded, so cost rises with width until subtree size caps it.
    let doc = json(5, 8).compile();
    for width in [20, 80, 1_280, 20_480, 100_000] {
        group.bench_with_input(
            BenchmarkId::new("json_width", width),
            &width,
            |b, &width| b.iter(|| doc.render(2, width)),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_compile_scaling, bench_render_scaling);
criterion_main!(benches);
