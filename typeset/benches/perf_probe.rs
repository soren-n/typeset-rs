//! Performance probe: scalable workload generators plus a tiny measurement
//! harness, used to check the pipeline's asymptotics and to give CPU/memory
//! profilers a long-running, representative target.
//!
//! Usage: cargo bench -p typeset --bench `perf_probe` -- WORKLOAD SIZE [key=val ...]
//!   keys: d=DEPTH width=W iters=K phase=compile|render|all loop=1
//!
//! Prints one CSV line per run:
//!   `workload,n,d,width,build_ns,compile_ns,render_ns,output_bytes`

use std::time::Instant;
use typeset::*;
use workloads::{deepgrp, fixed, json, lines, nestwide, packs, wide};

mod workloads;

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
    // `cargo bench` passes `--bench` to a harness-less target; ignore it.
    let mut argv = std::env::args().skip(1).filter(|a| a != "--bench");
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

fn build(args: &Args) -> Layout {
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
                drop(std::hint::black_box(layout.compile()));
            },
            _ => {
                let doc = layout.compile();
                loop {
                    std::hint::black_box(doc.render(2, args.width));
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
            let d = input.compile();
            best = best.min(t.elapsed().as_nanos());
            doc = Some(d);
        }
        compile_ns = best;
    }

    let mut render_ns = 0u128;
    let mut out_len = 0usize;
    if args.phase == "render" || args.phase == "all" {
        let doc = doc.unwrap_or_else(|| layout.compile());
        let mut best = u128::MAX;
        for _ in 0..args.iters {
            let t = Instant::now();
            let out = doc.render(2, args.width);
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
