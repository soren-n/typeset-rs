use criterion::{black_box, criterion_group, criterion_main, Criterion};
use typeset::{comp, compile, compile_safe, grp, nest, text};

fn create_deep_layout(depth: usize) -> Box<typeset::Layout> {
    let mut layout = text("base".to_string());

    for i in 0..depth {
        layout = nest(grp(comp(text(format!("level_{}", i)), layout, true, false)));
    }

    layout
}

fn create_wide_layout(width: usize) -> Box<typeset::Layout> {
    let mut layout = text("first".to_string());

    for i in 1..width {
        layout = comp(layout, text(format!("item_{}", i)), false, false);
    }

    layout
}

fn bench_compile_original(c: &mut Criterion) {
    c.bench_function("compile_original_deep_50", |b| {
        let layout = create_deep_layout(50);
        b.iter(|| compile(black_box(layout.clone())))
    });

    c.bench_function("compile_original_wide_100", |b| {
        let layout = create_wide_layout(100);
        b.iter(|| compile(black_box(layout.clone())))
    });
}

fn bench_compile_safe(c: &mut Criterion) {
    c.bench_function("compile_safe_deep_50", |b| {
        let layout = create_deep_layout(50);
        b.iter(|| compile_safe(black_box(layout.clone())).unwrap())
    });

    c.bench_function("compile_safe_wide_100", |b| {
        let layout = create_wide_layout(100);
        b.iter(|| compile_safe(black_box(layout.clone())).unwrap())
    });
}

fn bench_memory_stress(c: &mut Criterion) {
    // Test with larger layouts to stress memory allocation
    c.bench_function("compile_original_stress", |b| {
        let layout = create_deep_layout(100);
        b.iter(|| compile(black_box(layout.clone())))
    });

    c.bench_function("compile_safe_stress", |b| {
        let layout = create_deep_layout(100);
        b.iter(|| compile_safe(black_box(layout.clone())).unwrap())
    });
}

criterion_group!(
    benches,
    bench_compile_original,
    bench_compile_safe,
    bench_memory_stress
);
criterion_main!(benches);
