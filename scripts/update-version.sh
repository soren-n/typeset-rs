#!/bin/bash
# Bump the workspace version ahead of cutting a release tag:
#   ./scripts/update-version.sh 5.0.0
# then update CHANGELOG.md, commit, tag v5.0.0 and push (see release.yml).
set -euo pipefail
[ $# -eq 1 ] || { echo "usage: $0 <version>" >&2; exit 1; }
cd "$(git rev-parse --show-toplevel)"
# The shared [workspace.package] version and the versions the workspace's
# own crates are required at (path dependencies need one to publish).
sed -i.bak -E \
    -e "s/^version = \".*\"/version = \"$1\"/" \
    -e "s/^(typeset(-parser)? = \{ version = )\"[^\"]*\"/\1\"$1\"/" \
    Cargo.toml
rm -f Cargo.toml.bak
cargo update --workspace --offline >/dev/null
grep -n "$1" Cargo.toml
