#!/bin/bash

# Script to fix code quality issues before enabling git hooks

set -e

echo "Fixing code quality issues..."

# 1. Fix formatting
echo "Applying code formatting..."
cargo fmt
echo "Code formatting applied"

# 2. Try to fix clippy issues automatically where possible
echo "Attempting to fix clippy suggestions..."
cargo clippy --fix --allow-dirty --allow-staged
echo "Clippy auto-fixes applied"

echo "Code quality fixes completed."
echo "You may need to manually fix remaining clippy warnings that couldn't be auto-fixed."