use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use typeset::*;

// Benchmark data structures for different complexity levels
fn create_simple_layout() -> Box<Layout> {
    join_with_spaces(vec![
        text_str("Hello"),
        text_str("World"),
        text_str("from"),
        text_str("typeset"),
    ])
}

fn create_nested_layout(depth: usize) -> Box<Layout> {
    let mut layout = text_str("base");
    for i in 0..depth {
        layout = nest(comp(text_str(&format!("level_{}", i)), layout, true, false));
    }
    layout
}

fn create_wide_layout(width: usize) -> Box<Layout> {
    let items: Vec<_> = (0..width)
        .map(|i| text_str(&format!("item_{}", i)))
        .collect();
    join_with_spaces(items)
}

fn create_json_like_layout(size: usize) -> Box<Layout> {
    let entries: Vec<_> = (0..size)
        .map(|i| {
            unpad(
                unpad(text_str(&format!("\"key_{}\"", i)), text_str(": ")),
                text_str(&format!("\"value_{}\"", i)),
            )
        })
        .collect();

    braces(nest(join_with_lines(
        vec![null()]
            .into_iter()
            .chain(entries.into_iter().enumerate().map(|(i, entry)| {
                if i == 0 {
                    entry
                } else {
                    unpad(comma(), pad(null(), entry))
                }
            }))
            .chain(vec![null()])
            .collect(),
    )))
}

// Benchmark layout construction
fn bench_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("construction");

    group.bench_function("simple", |b| b.iter(|| create_simple_layout()));

    for depth in [5, 10, 20].iter() {
        group.bench_with_input(BenchmarkId::new("nested", depth), depth, |b, &depth| {
            b.iter(|| create_nested_layout(depth))
        });
    }

    for width in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("wide", width), width, |b, &width| {
            b.iter(|| create_wide_layout(width))
        });
    }

    for size in [5, 10, 25].iter() {
        group.bench_with_input(BenchmarkId::new("json_like", size), size, |b, &size| {
            b.iter(|| create_json_like_layout(size))
        });
    }

    group.finish();
}

// Benchmark compilation step
fn bench_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation");

    let simple = create_simple_layout();
    group.bench_function("simple", |b| b.iter(|| compile(simple.clone())));

    for depth in [5, 10, 20].iter() {
        let layout = create_nested_layout(*depth);
        group.bench_with_input(BenchmarkId::new("nested", depth), depth, |b, _| {
            b.iter(|| compile(layout.clone()))
        });
    }

    for width in [10, 50, 100].iter() {
        let layout = create_wide_layout(*width);
        group.bench_with_input(BenchmarkId::new("wide", width), width, |b, _| {
            b.iter(|| compile(layout.clone()))
        });
    }

    for size in [5, 10, 25].iter() {
        let layout = create_json_like_layout(*size);
        group.bench_with_input(BenchmarkId::new("json_like", size), size, |b, _| {
            b.iter(|| compile(layout.clone()))
        });
    }

    group.finish();
}

// Benchmark rendering step
fn bench_rendering(c: &mut Criterion) {
    let mut group = c.benchmark_group("rendering");

    let simple_doc = compile(create_simple_layout());
    group.bench_function("simple", |b| b.iter(|| render(simple_doc.clone(), 2, 80)));

    // Test rendering at different widths to show width independence
    let complex_doc = compile(create_json_like_layout(20));
    for width in [20, 40, 80, 120].iter() {
        group.bench_with_input(
            BenchmarkId::new("different_widths", width),
            width,
            |b, &width| b.iter(|| render(complex_doc.clone(), 2, width)),
        );
    }

    group.finish();
}

// Benchmark end-to-end performance
fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");

    group.bench_function("simple", |b| {
        b.iter(|| {
            let layout = create_simple_layout();
            let doc = compile(layout);
            render(doc, 2, 80)
        })
    });

    for size in [5, 10, 20].iter() {
        group.bench_with_input(BenchmarkId::new("json_like", size), size, |b, &size| {
            b.iter(|| {
                let layout = create_json_like_layout(size);
                let doc = compile(layout);
                render(doc, 2, 80)
            })
        });
    }

    // Test the convenience function
    group.bench_function("convenience_api", |b| {
        b.iter(|| {
            let layout = create_simple_layout();
            format_layout(layout, 2, 80)
        })
    });

    group.finish();
}

// Benchmark memory efficiency by measuring multiple renders
fn bench_reuse_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("reuse_efficiency");

    let doc = compile(create_json_like_layout(15));

    group.bench_function("compile_once_render_multiple", |b| {
        b.iter(|| {
            // This demonstrates the efficiency of the two-phase approach
            for width in [20, 40, 60, 80, 100] {
                let _ = render(doc.clone(), 2, width);
            }
        })
    });

    group.bench_function("compile_each_time", |b| {
        b.iter(|| {
            // This shows the cost of compiling each time
            let layout = create_json_like_layout(15);
            for width in [20, 40, 60, 80, 100] {
                let doc = compile(layout.clone());
                let _ = render(doc, 2, width);
            }
        })
    });

    group.finish();
}

// Benchmark different layout combinators
fn bench_combinators(c: &mut Criterion) {
    let mut group = c.benchmark_group("combinators");

    let base = text_str("content");

    group.bench_function("text", |b| b.iter(|| text_str("benchmark text")));

    group.bench_function("comp_padded", |b| {
        b.iter(|| comp(base.clone(), base.clone(), true, false))
    });

    group.bench_function("comp_unpadded", |b| {
        b.iter(|| comp(base.clone(), base.clone(), false, false))
    });

    group.bench_function("line", |b| b.iter(|| line(base.clone(), base.clone())));

    group.bench_function("nest", |b| b.iter(|| nest(base.clone())));

    group.bench_function("pack", |b| b.iter(|| pack(base.clone())));

    group.bench_function("fix", |b| b.iter(|| fix(base.clone())));

    group.bench_function("grp", |b| b.iter(|| grp(base.clone())));

    group.bench_function("seq", |b| b.iter(|| seq(base.clone())));

    group.finish();
}

criterion_group!(
    benches,
    bench_construction,
    bench_compilation,
    bench_rendering,
    bench_end_to_end,
    bench_reuse_efficiency,
    bench_combinators
);
criterion_main!(benches);
