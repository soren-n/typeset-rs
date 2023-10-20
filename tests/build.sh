# Clean up
rm -fr _build
(cd tester && dune clean)
(cd unit && cargo clean)

# Build
mkdir _build
(cd tester && dune build)
(cd unit && cargo build)
cp tester/_build/default/bin/main.exe _build/tester
cp unit/target/debug/unit _build/unit