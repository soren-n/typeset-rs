#!/bin/sh
# Build the differential harness: the OCaml QCheck tester and oracle, and the
# Rust driver, staged under _build/.
set -e
mkdir -p _build
(cd tester && dune build)
(cd .. && cargo build -p typeset-differential)
cp -f tester/_build/default/bin/main.exe _build/tester
cp -f tester/_build/default/bin/oracle.exe _build/oracle
cp -f ../target/debug/typeset-differential _build/differential
