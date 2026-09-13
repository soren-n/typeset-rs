//! Workload generators shared by the scaling bench and the probes. Each
//! target uses a subset.
#![allow(dead_code)]

use typeset::*;

/// Left-leaning chain of `n` words joined by `brk` compositions.
pub fn chain(n: usize, brk: Break) -> Layout {
    let mut layout = text("w0");
    for i in 1..n {
        layout = comp(layout, text(format!("w{i}")), Pad::Padded, brk);
    }
    layout
}

/// Breakable chain of `n` words.
pub fn wide(n: usize) -> Layout {
    chain(n, Break::Breakable)
}

/// One run of `n` words.
pub fn fixed(n: usize) -> Layout {
    fix(chain(n, Break::Fixed))
}

/// `n` hard lines (a long document spine).
pub fn lines(n: usize) -> Layout {
    let mut layout = text("l0");
    for i in 1..n {
        layout = line(layout, text(format!("l{i}")));
    }
    layout
}

/// nest^d over a breakable chain of `m` words: stresses distributing the
/// nest wrappers over every leaf and factoring them back out.
pub fn nestwide(d: usize, m: usize) -> Layout {
    let mut layout = wide(m);
    for _ in 0..d {
        layout = nest(layout);
    }
    layout
}

/// grp(nest(...))^d around a short chain: deep scope nesting.
pub fn deepgrp(d: usize) -> Layout {
    let mut layout = wide(4);
    for _ in 0..d {
        layout = grp(nest(layout));
    }
    layout
}

/// `n` pack-aligned short chains: stresses the renderer's pack marks.
pub fn packs(n: usize) -> Layout {
    let mut layout = pack(wide(4));
    for _ in 1..n {
        layout = comp(layout, pack(wide(4)), Pad::Padded, Break::Breakable);
    }
    layout
}

/// Balanced JSON-ish tree: objects of `fan` entries, `d` levels deep, with
/// the grp/seq/nest structure a formatter emits. Leaf count is `fan^d`.
pub fn json(d: usize, fan: usize) -> Layout {
    fn value(d: usize, fan: usize, i: usize) -> Layout {
        if d == 0 {
            return text(format!("\"value_{i}\""));
        }
        let mut body: Option<Layout> = None;
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
        let body = body.expect("fan > 0");
        grp(comp(
            comp(text("{"), seq(nest(body)), Pad::Unpadded, Break::Breakable),
            text("}"),
            Pad::Unpadded,
            Break::Breakable,
        ))
    }
    value(d, fan, 0)
}

/// Layout node count of [`json`]`(d, fan)`: a leaf is one node; a level is
/// `fan` entries of two nodes plus the value, three nodes per join, and
/// seven for the braces and wrappers.
pub fn json_nodes(d: usize, fan: usize) -> u64 {
    let fan = fan as u64;
    let mut count = 1u64;
    for _ in 0..d {
        count = fan * (2 + count) + (fan - 1) * 3 + 7;
    }
    count
}
