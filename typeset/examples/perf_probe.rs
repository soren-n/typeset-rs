//! Performance probe: scalable workload generators plus a tiny measurement
//! harness, used to check the pipeline's asymptotics and to give CPU/memory
//! profilers a long-running, representative target.
//!
//! Usage: perf_probe WORKLOAD SIZE [key=val ...]
//!   keys: d=DEPTH width=W iters=K phase=compile|render|all loop=1
//!
//! Prints one CSV line per run:
//!   workload,n,d,width,build_ns,compile_ns,render_ns,output_bytes

use std::time::Instant;
use typeset::*;

fn chain(n: usize, brk: Break) -> Box<Layout> {
    let mut layout = text("w0");
    for i in 1..n {
        layout = comp(layout, text(format!("w{i}")), Pad::Padded, brk);
    }
    layout
}

/// Right-leaning breakable comp chain of `n` words.
fn wide(n: usize) -> Box<Layout> {
    chain(n, Break::Breakable)
}

/// One fix run of `n` words.
fn fixed(n: usize) -> Box<Layout> {
    fix(chain(n, Break::Fixed))
}

/// `n` hard lines (document spine).
fn lines(n: usize) -> Box<Layout> {
    let mut layout = text("l0");
    for i in 1..n {
        layout = line(layout, text(format!("l{i}")));
    }
    layout
}

/// nest^d over a breakable chain of `m` words: stresses distributing the
/// nest wrappers over every leaf and re-factoring them back out.
fn nestwide(d: usize, m: usize) -> Box<Layout> {
    let mut layout = chain(m, Break::Breakable);
    for _ in 0..d {
        layout = nest(layout);
    }
    layout
}

/// grp(nest(...))^d around a small chain: deep scope nesting.
fn deepgrp(d: usize) -> Box<Layout> {
    let mut layout = chain(4, Break::Breakable);
    for _ in 0..d {
        layout = grp(nest(layout));
    }
    layout
}

/// `n` pack-aligned groups of short chains: stresses the renderer's marks map.
fn packs(n: usize) -> Box<Layout> {
    let mut layout = pack(chain(4, Break::Breakable));
    for _ in 1..n {
        layout = comp(
            layout,
            pack(chain(4, Break::Breakable)),
            Pad::Padded,
            Break::Breakable,
        );
    }
    layout
}

/// Balanced JSON-ish tree: objects of `fan` entries, `d` levels deep, with
/// grp/seq/nest structure like a real formatter would emit.
fn json(d: usize, fan: usize) -> Box<Layout> {
    fn value(d: usize, fan: usize, i: usize) -> Box<Layout> {
        if d == 0 {
            return text(format!("\"value_{i}\""));
        }
        let mut body: Option<Box<Layout>> = None;
        for k in 0..fan {
            let entry = comp(
                text(format!("\"key_{k}\":")),
                value(d - 1, fan, k),
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
        let body = body.unwrap_or_else(null);
        grp(comp(
            comp(text("{"), seq(nest(body)), Pad::Unpadded, Break::Breakable),
            text("}"),
            Pad::Unpadded,
            Break::Breakable,
        ))
    }
    value(d, fan, 0)
}

struct Args {
    workload: String,
    n: usize,
    d: usize,
    width: usize,
    iters: usize,
    phase: String,
    forever: bool,
}

fn parse_args() -> Args {
    let mut argv = std::env::args().skip(1);
    let workload = argv.next().expect("workload name");
    let n: usize = argv.next().expect("size").parse().expect("size int");
    let mut args = Args {
        workload,
        n,
        d: 8,
        width: 80,
        iters: 3,
        phase: "all".to_string(),
        forever: false,
    };
    for kv in argv {
        let (k, v) = kv.split_once('=').expect("key=val");
        match k {
            "d" => args.d = v.parse().unwrap(),
            "width" => args.width = v.parse().unwrap(),
            "iters" => args.iters = v.parse().unwrap(),
            "phase" => args.phase = v.to_string(),
            "loop" => args.forever = v == "1",
            _ => panic!("unknown key {k}"),
        }
    }
    args
}

fn build(args: &Args) -> Box<Layout> {
    match args.workload.as_str() {
        "wide" => wide(args.n),
        "fixed" => fixed(args.n),
        "lines" => lines(args.n),
        "nestwide" => nestwide(args.d, args.n),
        "deepgrp" => deepgrp(args.n),
        "packs" => packs(args.n),
        "json" => json(args.d, args.n),
        other => panic!("unknown workload {other}"),
    }
}

fn main() {
    let args = parse_args();

    let t0 = Instant::now();
    let layout = build(&args);
    let build_ns = t0.elapsed().as_nanos();

    if args.forever {
        // Endless loop over the requested phase, for attaching a profiler.
        match args.phase.as_str() {
            "compile" => loop {
                let layout = build(&args);
                std::hint::black_box(compile(layout));
            },
            _ => {
                let doc = compile(layout);
                loop {
                    std::hint::black_box(render(&doc, 2, args.width));
                }
            }
        }
    }

    // Each iteration compiles a fresh clone; the clone happens outside the
    // timed region, so compile_ns is the best-of-iters compile time alone.
    let mut compile_ns = 0u128;
    let mut doc = None;
    if args.phase == "compile" || args.phase == "all" {
        let mut best = u128::MAX;
        for _ in 0..args.iters {
            let input = layout.clone();
            let t = Instant::now();
            let d = compile(input);
            best = best.min(t.elapsed().as_nanos());
            doc = Some(d);
        }
        compile_ns = best;
    }

    let mut render_ns = 0u128;
    let mut out_len = 0usize;
    if args.phase == "render" || args.phase == "all" {
        let doc = doc.unwrap_or_else(|| compile(layout));
        let mut best = u128::MAX;
        for _ in 0..args.iters {
            let t = Instant::now();
            let out = render(&doc, 2, args.width);
            best = best.min(t.elapsed().as_nanos());
            out_len = out.len();
            std::hint::black_box(out);
        }
        render_ns = best;
    }

    println!(
        "{},{},{},{},{},{},{},{}",
        args.workload, args.n, args.d, args.width, build_ns, compile_ns, render_ns, out_len
    );
}
