# Git Hooks Configuration

## Overview

The project uses comprehensive pre-commit hooks to enforce code quality and ensure all tests pass before commits are allowed. This prevents broken code from entering the repository and maintains consistency across the codebase.

## Configured Hooks

### Code Quality Checks
1. **Formatting**: `cargo fmt --check`
   - **Requirement**: Must pass (blocking)
   - **Purpose**: Enforces consistent code style
   - **Auto-fix**: Run `cargo fmt` or `./scripts/fix-code-quality.sh`

2. **Linting**: `cargo clippy --all-targets --all-features`
   - **Requirement**: Warnings allowed, errors block commits
   - **Purpose**: Catches common mistakes and suggests improvements
   - **Auto-fix**: Many issues can be resolved with `./scripts/fix-code-quality.sh`

3. **Type Checking**: `cargo check --all-targets --all-features`
   - **Requirement**: Must pass (blocking)
   - **Purpose**: Ensures type safety across all build targets
   - **Fix**: Address compilation errors manually

### Testing Requirements
4. **Rust Testing**: `cargo test --all --all-features`
   - **Requirement**: All tests must pass (blocking)
   - **Coverage**: Unit tests, integration tests, doc tests

5. **OCaml Property-Based Testing**
   - **Requirement**: All OCaml tests must pass (blocking)
   - **Auto-setup**: Installs missing dependencies automatically
   - **Dependencies**: `qcheck`, `typeset` OCaml packages

## Prerequisites

### System Requirements
- **Rust**: Stable toolchain with rustfmt and clippy components
- **OCaml**: OCaml compiler and opam package manager
- **Git**: Version control system with hooks support

### OCaml Dependencies (Auto-Installed)
The hooks automatically install missing OCaml packages:
```bash
opam install qcheck typeset
```

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
1. **Staging Check**: Hooks run on staged files only
2. **Formatting**: Code must be properly formatted
3. **Linting**: No clippy errors allowed (warnings OK)
4. **Type Check**: All code must compile cleanly
5. **Rust Tests**: All Rust unit and integration tests must pass
6. **OCaml Tests**: Property-based tests must pass
7. **Commit**: Only proceeds if all checks pass

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

**⚠️ Important**: 
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
chmod +x .git/hooks/pre-commit
```

**Formatting conflicts**:
```bash
# Fix formatting first
cargo fmt
git add .
git commit -m "fix: resolve formatting issues"
```

### Hook Installation
If hooks aren't running, ensure they're properly installed:
```bash
# Check if pre-commit hook exists and is executable
ls -la .git/hooks/pre-commit

# If missing, hooks may need to be reinstalled
# (This is typically handled by the repository setup)
```