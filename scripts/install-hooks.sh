#!/bin/bash
# Install the repository's git hooks by pointing git at the tracked .githooks
# directory. Run once after cloning:
#
#   ./scripts/install-hooks.sh

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

chmod +x .githooks/*
git config core.hooksPath .githooks

echo "Installed: core.hooksPath -> .githooks"
echo "Hooks active:"
ls -1 .githooks | sed 's/^/  /'
echo
echo "To bypass a hook for one commit: git commit --no-verify"
echo "To uninstall: git config --unset core.hooksPath"
