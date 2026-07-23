//! Allocation probe: counts heap traffic (allocs/frees/reallocs/bytes) for
//! each phase — build, clone, drop, compile, render — via a counting global
//! allocator. Companion to `perf_probe.rs`; used to attribute the
//! allocator-bound compile profile.
//!
//! Usage: alloc_probe WORKLOAD SIZE [d=DEPTH] [width=W]

// The counting allocator is the one place the workspace needs unsafe; it just
// forwards to `System` around atomic counters.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout as AllocLayout, System};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Instant;
use typeset::*;

struct Counting;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static FREES: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: AllocLayout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: AllocLayout) {
        FREES.fetch_add(1, Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: AllocLayout, new_size: usize) -> *mut u8 {
        REALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(new_size as u64, Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[derive(Copy, Clone)]
struct Snap {
    allocs: u64,
    frees: u64,
    reallocs: u64,
    bytes: u64,
}

fn snap() -> Snap {
    Snap {
        allocs: ALLOCS.load(Relaxed),
        frees: FREES.load(Relaxed),
        reallocs: REALLOCS.load(Relaxed),
        bytes: BYTES.load(Relaxed),
    }
}

fn report(label: &str, before: Snap, elapsed_ns: u128, nodes: u64) {
    let after = snap();
    let (a, f, r, b) = (
        after.allocs - before.allocs,
        after.frees - before.frees,
        after.reallocs - before.reallocs,
        after.bytes - before.bytes,
    );
    println!(
        "{label}: {a} allocs ({:.2}/node), {f} frees ({:.2}/node), {r} reallocs, \
         {:.1} MiB, {:.2} ms",
        a as f64 / nodes as f64,
        f as f64 / nodes as f64,
        b as f64 / (1024.0 * 1024.0),
        elapsed_ns as f64 / 1e6,
    );
}

fn chain(n: usize, brk: Break) -> Box<Layout> {
    let mut layout = text("w0");
    for i in 1..n {
        layout = comp(layout, text(format!("w{i}")), Pad::Padded, brk);
    }
    layout
}

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

fn main() {
    let mut argv = std::env::args().skip(1);
    let workload = argv.next().expect("workload");
    let n: usize = argv.next().expect("size").parse().unwrap();
    let mut d = 5usize;
    let mut width = 80usize;
    for kv in argv {
        let (k, v) = kv.split_once('=').expect("key=val");
        match k {
            "d" => d = v.parse().unwrap(),
            "width" => width = v.parse().unwrap(),
            _ => panic!("unknown key {k}"),
        }
    }

    // Node counts derived from the generators: chain(n) = n texts + (n-1)
    // comps; json adds per level: fan entries (text + comp) + comma fixes +
    // grp/seq/nest/braces.
    let (layout, nodes): (Box<Layout>, u64) = match workload.as_str() {
        "wide" => (chain(n, Break::Breakable), (2 * n - 1) as u64),
        "fixed" => (fix(chain(n, Break::Fixed)), (2 * n) as u64),
        "json" => {
            // nodes(0) = 1; nodes(k) = fan * (2 + nodes(k-1)) + (fan-1)*2 + 6
            let mut count = 1u64;
            for _ in 0..d {
                count = (n as u64) * (2 + count) + (n as u64 - 1) * 2 + 6;
            }
            (json(d, n), count)
        }
        other => panic!("unknown workload {other}"),
    };
    println!("workload={workload} n={n} d={d} width={width} tree_nodes={nodes}");

    let before = snap();
    let t = Instant::now();
    let cloned = layout.clone();
    report("clone ", before, t.elapsed().as_nanos(), nodes);

    let before = snap();
    let t = Instant::now();
    drop(cloned);
    report("drop  ", before, t.elapsed().as_nanos(), nodes);

    let before = snap();
    let t = Instant::now();
    let doc = compile(layout);
    report("compile", before, t.elapsed().as_nanos(), nodes);

    let before = snap();
    let t = Instant::now();
    let out = render(&doc, 2, width);
    report("render", before, t.elapsed().as_nanos(), nodes);
    std::hint::black_box(out);
}
