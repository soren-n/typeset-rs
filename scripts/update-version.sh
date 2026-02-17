#!/bin/bash
set -euo pipefail

# Update version script for semantic-release
# Usage: ./scripts/update-version.sh <new-version>

if [ $# -ne 1 ]; then
    echo "Usage: $0 <new-version>"
    exit 1
fi

NEW_VERSION="$1"

echo "Updating version to $NEW_VERSION"

# Update workspace version in root Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update inter-workspace dependency versions in root Cargo.toml
sed -i.bak "s/typeset = { version = \".*\", path = \"typeset\" }/typeset = { version = \"$NEW_VERSION\", path = \"typeset\" }/" Cargo.toml
rm -f Cargo.toml.bak

sed -i.bak "s/typeset-parser = { version = \".*\", path = \"typeset-parser\" }/typeset-parser = { version = \"$NEW_VERSION\", path = \"typeset-parser\" }/" Cargo.toml
rm -f Cargo.toml.bak

echo "Successfully updated version to $NEW_VERSION"
