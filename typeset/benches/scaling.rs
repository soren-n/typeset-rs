//! Scaling benchmarks: compile and render at sizes large enough to expose
//! asymptotics (the `layout_performance` bench covers small-input latency).
//!
//! Workloads mirror `examples/perf_probe.rs`, which is the profiling companion
//! to this bench (run it under `sample`/`samply` or `/usr/bin/time -l`).

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use typeset::*;

/// Right-leaning breakable comp chain of `n` words.
fn wide(n: usize) -> Box<Layout> {
    let mut layout = text("w0");
    for i in 1..n {
        layout = comp(layout, text(format!("w{i}")), Pad::Padded, Break::Breakable);
    }
    layout
}

/// nest^d over a breakable chain of `m` words: stresses distributing nest
/// wrappers over every leaf (compile cost is O(m * d)).
fn nestwide(d: usize, m: usize) -> Box<Layout> {
    let mut layout = wide(m);
    for _ in 0..d {
        layout = nest(layout);
    }
    layout
}

/// `n` pack-aligned groups: stresses the renderer's pack marks map.
fn packs(n: usize) -> Box<Layout> {
    let group = |i: usize| {
        pack(comp(
            text(format!("k{i}")),
            text(format!("v{i}")),
            Pad::Padded,
            Break::Breakable,
        ))
    };
    let mut layout = group(0);
    for i in 1..n {
        layout = comp(layout, group(i), Pad::Padded, Break::Breakable);
    }
    layout
}

/// Balanced JSON-ish tree: objects of `fan` entries, `d` levels deep, with the
/// grp/seq/nest structure a real formatter emits. Leaf count is `fan^d`.
fn json(d: usize, fan: usize) -> Box<Layout> {
    if d == 0 {
        return text("\"value\"");
    }
    let mut body: Option<Box<Layout>> = None;
    for k in 0..fan {
        let entry = comp(
            text(format!("\"key_{k}\":")),
            json(d - 1, fan),
            Pad::Padded,
            Break::Breakable,
        );
        body = Some(match body {
            None => entry,
            Some(prev) => comp(
                comp(prev, text(","), Pad::Unpadded, Break::Fixed),
                entry,
                Pad::Padded,
                Break::Breakable,
            ),
        });
    }
    grp(comp(
        comp(
            text("{"),
            seq(nest(body.expect("fan > 0"))),
            Pad::Unpadded,
            Break::Breakable,
        ),
        text("}"),
        Pad::Unpadded,
        Break::Breakable,
    ))
}

fn bench_compile_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile_scaling");
    group.sample_size(20);

    for n in [1_000, 8_000, 64_000] {
        let layout = wide(n);
        group.bench_with_input(BenchmarkId::new("wide", n), &n, |b, _| {
            b.iter(|| compile(layout.clone()))
        });
    }
    for d in [3, 4, 5] {
        let layout = json(d, 8);
        let leaves = 8usize.pow(d as u32);
        group.bench_with_input(BenchmarkId::new("json_leaves", leaves), &d, |b, _| {
            b.iter(|| compile(layout.clone()))
        });
    }
    for d in [64, 256, 1024] {
        let layout = nestwide(d, 1_000);
        group.bench_with_input(BenchmarkId::new("nestwide_1k_words", d), &d, |b, _| {
            b.iter(|| compile(layout.clone()))
        });
    }

    group.finish();
}

fn bench_render_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_scaling");

    for n in [1_000, 8_000, 64_000] {
        let doc = compile(wide(n));
        group.bench_with_input(BenchmarkId::new("wide", n), &n, |b, _| {
            b.iter(|| render(&doc, 2, 80))
        });
    }
    for n in [1_000, 8_000, 64_000] {
        let doc = compile(packs(n));
        group.bench_with_input(BenchmarkId::new("packs", n), &n, |b, _| {
            b.iter(|| render(&doc, 2, 80))
        });
    }

    // Width sweep on a grp/seq-heavy document: the renderer's look-ahead is
    // width-bounded, so cost rises with width until subtree size caps it.
    let doc = compile(json(5, 8));
    for width in [20, 80, 1_280, 20_480, 100_000] {
        group.bench_with_input(
            BenchmarkId::new("json_width", width),
            &width,
            |b, &width| b.iter(|| render(&doc, 2, width)),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_compile_scaling, bench_render_scaling);
criterion_main!(benches);
