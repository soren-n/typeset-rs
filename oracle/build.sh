#!/bin/sh
# Build the oracle harness: the OCaml QCheck tester and one-shot oracle, and
# the Rust driver they compare against, staged under _build/. Then:
#   ./_build/tester                       the QCheck identity suite
#   ./compare.sh '<layout dsl>' tab width  one expression, both implementations
#   ./_build/oracle '<layout dsl>' tab width  the reference alone
set -e
mkdir -p _build
(cd tester && dune build)
(cd .. && cargo build -p typeset-differential)
cp -f tester/_build/default/bin/main.exe _build/tester
cp -f tester/_build/default/bin/oracle.exe _build/oracle
cp -f ../target/debug/typeset-differential _build/differential
