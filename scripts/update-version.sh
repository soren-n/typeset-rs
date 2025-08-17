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

# Update typeset/Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" typeset/Cargo.toml
rm -f typeset/Cargo.toml.bak

# Update typeset-parser/Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" typeset-parser/Cargo.toml
rm -f typeset-parser/Cargo.toml.bak

# Update typeset dependency version in typeset-parser/Cargo.toml
sed -i.bak "s/typeset = { version = \".*\", path = \"..\/typeset\" }/typeset = { version = \"$NEW_VERSION\", path = \"..\/typeset\" }/" typeset-parser/Cargo.toml
rm -f typeset-parser/Cargo.toml.bak

echo "Successfully updated version to $NEW_VERSION"