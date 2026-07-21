#!/usr/bin/env python3
"""Differential fuzzer: compare the Rust and OCaml renderers on random layouts.

The QCheck suite in tester/ generates layouts uniformly and so almost never
produces the grp/seq nestings where the two implementations disagreed. This
generator is deliberately biased toward stacked grp/seq wrappers and renders at
narrow widths, where breaking decisions actually differ.

Usage: fuzz.py [iterations] [seed]
"""

import random
import subprocess
import sys

UNARY = ["fix", "grp", "seq", "nest", "pack"]
BINARY = ["@", "@@", "&", "+", "!&", "!+"]
WORDS = ["a", "b", "c", "ab", "abc", "abcd", "wxyz", "hello"]


def gen(rng, depth):
    if depth <= 0:
        return rng.choice(['"%s"' % rng.choice(WORDS), "null"])
    roll = rng.random()
    if roll < 0.40:
        # Bias toward grp/seq: these drive the breaking decisions under test.
        op = rng.choice(["grp", "seq"] if rng.random() < 0.7 else UNARY)
        return "%s (%s)" % (op, gen(rng, depth - 1))
    return "(%s) %s (%s)" % (
        gen(rng, depth - 1),
        rng.choice(BINARY),
        gen(rng, depth - 1),
    )


def run(cmd):
    proc = subprocess.run(cmd, capture_output=True, text=True)
    return proc.stdout, proc.returncode


def main():
    iterations = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    rng = random.Random(seed)
    failures = 0
    for i in range(iterations):
        # Deeper trees interleave more grp/seq scopes, which is where the two
        # implementations' breaking decisions can diverge. Depth stays well
        # under the ~2000-level recursive-parser limit.
        expr = gen(rng, rng.randint(3, 9))
        width = str(rng.choice([1, 2, 3, 5, 8, 12, 20, 40, 80]))
        tab = str(rng.choice([0, 1, 2, 4, 8]))
        ocaml, oc_rc = run(["./_build/oracle", expr, tab, width])
        rust_raw, rs_rc = run(["./_build/unit", expr, tab, width])
        if oc_rc != 0 or rs_rc != 0:
            print("ERROR rc oc=%d rs=%d: %s" % (oc_rc, rs_rc, expr))
            failures += 1
            continue
        marker = "!!!!output!!!!\n"
        rust = rust_raw.split(marker, 1)[1] if marker in rust_raw else rust_raw
        if ocaml != rust:
            failures += 1
            print("=" * 60)
            print("expr:  %s\ntab:   %s\nwidth: %s" % (expr, tab, width))
            print("--- ocaml ---\n%s--- rust ---\n%s" % (ocaml, rust))
            if failures >= 5:
                print("stopping after 5 mismatches")
                break
    print("checked %d, mismatches %d" % (i + 1, failures))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
