//! Allocation probe: counts heap traffic (allocs/frees/reallocs/bytes) for
//! each phase — clone, drop, compile, render — via a counting global
//! allocator. Companion to `perf_probe`; used to attribute the
//! allocator-bound compile profile.
//!
//! ```text
//! cargo bench -p typeset --bench alloc_probe -- WORKLOAD SIZE [d=DEPTH] [width=W]
//! ```

// The counting allocator is the one place the workspace needs unsafe; it just
// forwards to `System` around atomic counters.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout as AllocLayout, System};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Instant;
use typeset::*;
use workloads::{chain, fixed, json, json_nodes};

mod workloads;

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

fn main() {
    // `cargo bench` passes `--bench` to a harness-less target; ignore it.
    let mut argv = std::env::args().skip(1).filter(|a| a != "--bench");
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

    // Node counts follow the generators: chain(n) is n texts and n-1 comps.
    let (layout, nodes): (Layout, u64) = match workload.as_str() {
        "wide" => (chain(n, Break::Breakable), (2 * n - 1) as u64),
        "fixed" => (fixed(n), (2 * n) as u64),
        "json" => (json(d, n), json_nodes(d, n)),
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
    let doc = layout.compile();
    report("compile", before, t.elapsed().as_nanos(), nodes);

    let before = snap();
    let t = Instant::now();
    let out = doc.render(2, width);
    report("render", before, t.elapsed().as_nanos(), nodes);
    std::hint::black_box(out);
}
