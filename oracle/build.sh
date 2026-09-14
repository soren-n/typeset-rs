#!/bin/sh
# Build the oracle harness into _build/: the OCaml tester and the Rust driver
# it renders through. Then:
#   ./_build/tester                              the QCheck identity suite
#   ./_build/tester '<layout dsl>' [tab width]    one expression, both implementations
#   ./_build/tester --reference '<layout dsl>' tab width
#                                                the reference's rendering, as a pinned case
#   ./pin.sh                                     re-pin typeset/tests/oracle.txt
set -e
mkdir -p _build
(cd tester && dune build)
(cd .. && cargo build -q -p typeset-oracle-driver)
cp -f tester/_build/default/bin/main.exe _build/tester
cp -f ../target/debug/typeset-oracle-driver _build/driver
