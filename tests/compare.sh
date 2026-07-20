#!/bin/sh
e="$1"; tab="${2:-2}"; width="${3:-80}"
o=$(./_build/oracle "$e" "$tab" "$width" 2>&1)
r=$(./_build/unit "$e" "$tab" "$width" 2>&1 | sed -n '/!!!!output!!!!/,$p' | tail -n +2)
if [ "$o" = "$r" ]; then echo "MATCH: $e"; else
  echo "DIFF:  $e"; echo "--- ocaml ---"; echo "$o"; echo "--- rust ---"; echo "$r"; fi
