# Git Hooks Configuration

## Overview

The project uses a pre-commit hook to enforce code quality and ensure all tests pass before commits are allowed. This prevents broken code from entering the repository and maintains consistency across the codebase.

The hook is not active in a fresh clone — run `./scripts/install-hooks.sh` once to enable it (see [Hook Installation](#hook-installation)).

## Configured Hooks

### Code Quality Checks
1. **Formatting**: `cargo fmt --check`
   - **Requirement**: Must pass (blocking)
   - **Purpose**: Enforces consistent code style
   - **Auto-fix**: Run `cargo fmt` or `./scripts/fix-code-quality.sh`

2. **Linting**: `cargo clippy --all-targets --all-features -- -D warnings`
   - **Requirement**: Must pass (blocking); warnings are treated as errors,
     matching CI
   - **Purpose**: Catches common mistakes and suggests improvements
   - **Auto-fix**: Many issues can be resolved with `./scripts/fix-code-quality.sh`

3. **Type Checking**: `cargo check --all-targets --all-features`
   - **Requirement**: Must pass (blocking)
   - **Purpose**: Ensures type safety across all build targets
   - **Fix**: Address compilation errors manually

4. **Doc Build**: `cargo doc --no-deps --all-features`
   - **Requirement**: Must pass (blocking)
   - **Purpose**: Catches broken intra-doc links (denied at the crate root),
     so stale doc references cannot merge

### Testing Requirements
5. **Rust Testing**: `cargo test --all --all-features`
   - **Requirement**: All tests must pass (blocking)
   - **Coverage**: Unit tests, integration tests, doc tests

6. **OCaml Property-Based Testing**
   - **Requirement**: All OCaml tests must pass (blocking)
   - **Setup**: dependencies must be installed manually (see Prerequisites)
   - **Dependencies**: `qcheck`, `typeset` OCaml packages
   - **Skip**: set `SKIP_OCAML=1` to bypass this step only

## Prerequisites

### System Requirements
- **Rust**: Stable toolchain with rustfmt and clippy components
- **OCaml**: OCaml compiler and opam package manager
- **Git**: Version control system with hooks support

### OCaml Dependencies (Manual)
Install the OCaml packages the property tests need:
```bash
opam install qcheck typeset
```

The hook does not install these for you. If `dune` is missing, or the tester
fails to build, the hook prints a warning and skips the OCaml suite rather than
blocking the commit — every other check still runs and still blocks.

If opam or OCaml are not installed, you'll need to install them manually:
```bash
# macOS
brew install opam
opam init

# Ubuntu/Debian  
apt-get install opam
opam init
```

## Quick Fix Script

### Automated Fixes
```bash
./scripts/fix-code-quality.sh
```

This script automatically fixes:
- Code formatting issues (`cargo fmt`)
- Auto-fixable clippy warnings (`cargo clippy --fix`)
- Import organization and other mechanical fixes

### Manual Fixes Required
Some issues require manual intervention:
- Logic errors caught by clippy
- Test failures
- Type errors
- Complex linting warnings

## Hook Workflow

### Pre-Commit Process
1. **Scope**: checks run against the working tree, not the staged snapshot
2. **Formatting**: Code must be properly formatted
3. **Linting**: No clippy warnings allowed (`-D warnings`)
4. **Type Check**: All code must compile cleanly
5. **Doc Build**: `cargo doc` must succeed (no broken intra-doc links)
6. **Rust Tests**: All Rust unit and integration tests must pass
7. **OCaml Tests**: Property-based tests must pass
8. **Commit**: Only proceeds if all checks pass

Because the checks read the working tree rather than the index, a commit that
stages only part of your changes is validated against everything present on
disk. A partially-staged commit can therefore pass the hook while not building
in isolation, which matters when bisecting. CI validates each pushed commit.

### Failure Handling
If any hook fails:
- **Commit is blocked** - changes are not committed
- **Error details** are displayed showing which check failed
- **Next steps** are suggested (run fix script, manual fixes, etc.)

## Bypassing Hooks (Not Recommended)

### Emergency Override
In rare cases where hooks must be bypassed:
```bash
git commit --no-verify -m "emergency fix: brief description"
```

**Important**: 
- Only use in genuine emergencies
- Follow up immediately with a proper commit that passes all hooks
- Document the reason in the commit message

### CI Will Still Enforce Quality
Even if hooks are bypassed locally, the CI pipeline will still:
- Run all the same quality checks
- Block PRs that don't meet standards
- Prevent broken code from being merged

## Troubleshooting

### Common Issues

**OCaml dependencies not found**:
```bash
# Initialize opam if first time
opam init
eval $(opam env)

# Install missing packages
opam install qcheck typeset
```

**Permission errors on hook scripts**:
```bash  
chmod +x .githooks/pre-commit
```

**Formatting conflicts**:
```bash
# Fix formatting first
cargo fmt
git add .
git commit -m "fix: resolve formatting issues"
```

### Hook Installation
Hooks live in the tracked `.githooks/` directory and are activated by pointing
git at it. Run once after cloning:
```bash
./scripts/install-hooks.sh
```

Verify it took effect:
```bash
git config core.hooksPath   # should print: .githooks
```

To bypass hooks for a single commit, use `git commit --no-verify`. To uninstall,
run `git config --unset core.hooksPath`.