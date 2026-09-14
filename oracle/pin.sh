#!/bin/sh
# Re-pins typeset/tests/oracle.txt: every case's expected block is
# regenerated from the OCaml reference (build the harness first: ./build.sh).
# Add a case as a `[tab width] layout` header and run this; the Rust test
# reads the file, and the hook and CI fail when a block is stale.
set -e
cd "$(dirname "$0")"
file=../typeset/tests/oracle.txt
tmp=$(mktemp)
while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
        '|'*) ;;
        '['*)
            printf '%s\n' "$line" >> "$tmp"
            spec=${line#[}
            dims=${spec%%]*}
            layout=${spec#*] }
            # shellcheck disable=SC2086
            ./_build/tester --reference "$layout" $dims >> "$tmp"
            ;;
        *) printf '%s\n' "$line" >> "$tmp" ;;
    esac
done < "$file"
mv "$tmp" "$file"
