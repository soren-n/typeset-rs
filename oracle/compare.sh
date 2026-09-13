#!/bin/sh
# Render one DSL expression with both implementations and diff the results.
# Usage: ./compare.sh '<layout dsl>' [tab] [width]   (after ./build.sh)
e="$1"; tab="${2:-2}"; width="${3:-80}"
o=$(./_build/oracle "$e" "$tab" "$width" 2>&1)
r=$(./_build/driver "$e" "$tab" "$width" 2>&1)
if [ "$o" = "$r" ]; then echo "MATCH: $e"; else
  echo "DIFF:  $e"; echo "--- ocaml ---"; echo "$o"; echo "--- rust ---"; echo "$r"; fi
