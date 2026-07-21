# CI/CD Pipeline

## Overview

The project uses GitHub Actions workflows for continuous integration, releases, and dependency management. Releases are cut explicitly by pushing a version tag; there is no automated version bumping.

## Workflows

### 1. CI Pipeline (`.github/workflows/ci.yml`)
**Triggers**: Every push and pull request

**Quality Gates**:
- **Formatting**: `cargo fmt --check` (must pass)
- **Linting**: `cargo clippy` (warnings allowed, errors blocked)  
- **Type Checking**: `cargo check --all-targets --all-features`
- **Testing**: Rust tests (`cargo test --all`)
- **Differential correctness** (`differential` job): builds the OCaml reference
  oracle (the `typeset` opam package) plus the Rust `unit` binary and compares
  their rendered output. Runs the grp/seq-biased differential fuzzer
  (`tests/fuzz.py`) and the QCheck oracle (`tests/run.sh`). This is the primary
  guard against silent renderer divergence, which uniform QCheck rarely catches.

**Matrix Testing**:
- Rust stable and MSRV (1.89.0)
- Multiple OS environments (if configured)

**Security & Compliance**:
- `cargo-audit`: Security vulnerability scanning
- `cargo-deny`: License and dependency policy enforcement
- Build verification and artifact generation

### 2. Release Pipeline (`.github/workflows/release.yml`)
**Triggers**: Pushing a `v*` tag (e.g. `v3.3.0`)

**Release Process**:
- Verifies the tag matches the `[workspace.package]` version in `Cargo.toml` and
  fails on mismatch
- Builds and runs the full test suite
- Publishes crates to crates.io in dependency order (`typeset-parser` first,
  then `typeset`, which depends on it)
- Creates a GitHub release whose body links to `CHANGELOG.md`

Version bumping and `CHANGELOG.md` are manual (see [Releasing](#releasing)
below); the workflow only publishes what the tag points at.

### 3. Dependencies Workflow (`.github/workflows/dependencies.yml`)
**Triggers**: Weekly schedule

**Automated Maintenance**:
- Updates Rust dependencies with automated PRs
- Security vulnerability scanning
- License compliance checking
- Dependency freshness monitoring

## Releasing

Releases are cut explicitly. Both crates share one version from
`[workspace.package]` in the root `Cargo.toml`.

```bash
# 1. Bump the workspace version (updates Cargo.toml + inter-crate deps)
./scripts/update-version.sh 3.3.0

# 2. Update CHANGELOG.md by hand for the new version

# 3. Commit the bump
git add Cargo.toml CHANGELOG.md
git commit -m "chore(release): 3.3.0"

# 4. Tag and push (tag must be v<version> and match Cargo.toml)
git tag v3.3.0
git push origin main --tags
```

Pushing the tag triggers `release.yml`, which verifies `v3.3.0` matches the
`Cargo.toml` version, builds, tests, publishes both crates to crates.io, and
creates the GitHub release. Follow [semver](https://semver.org/) when choosing
the number.

Conventional-commit-style messages (`feat:`, `fix:`, `docs:`, …) are still
encouraged for a readable history, but they no longer drive versioning — nothing
is bumped automatically.

## Development Workflow

### Standard Process
1. **Branch Creation**: Create feature branch with descriptive name
   ```bash
   git checkout -b feat/custom-indentation
   ```

2. **Development**: Make changes following conventional commit format
   ```bash
   git commit -m "feat: implement pack indentation for custom alignment"
   ```

3. **Push & CI**: Push triggers CI validation
   ```bash  
   git push origin feat/custom-indentation
   ```

4. **Pull Request**: Create PR for code review
   - CI must pass (formatting, linting, tests)
   - All quality gates enforced
   - Manual review process

5. **Merge**: Squash and merge to main after review. Merging does **not**
   publish anything — releases are cut separately by tagging (see
   [Releasing](#releasing)).

### Git Hooks (Local Development)
A pre-commit hook enforces quality before CI:
- Code formatting check
- Clippy linting  
- Type checking
- Complete test suite (including OCaml tests)

Enable it once per clone with `./scripts/install-hooks.sh`. OCaml dependencies
are not installed automatically — see [GIT_HOOKS.md](GIT_HOOKS.md).

**Quick fixes**:
```bash
./scripts/fix-code-quality.sh    # Auto-fix formatting and clippy issues
```

## Release Management

- **Trigger**: pushing a `v*` tag (see [Releasing](#releasing)).
- **Version guard**: the workflow aborts if the tag does not match the
  `Cargo.toml` version, so a forgotten bump fails loudly instead of shipping.
- **Dependency order**: `typeset-parser` is published first, then `typeset`
  (which depends on it); the workflow waits for the parser to appear on
  crates.io before publishing `typeset`.
- **Version synchronization**: both crates share the single `[workspace.package]`
  version.
- **Failure handling**: a crates.io publish is immutable — a bad release must be
  yanked and a new version tagged; the workflow can otherwise be re-run.

## Monitoring & Maintenance

### Quality Metrics
- Test coverage tracking
- Performance regression detection  
- Security vulnerability alerts
- Dependency staleness monitoring

### Automated Updates
- Weekly dependency updates via PRs
- Security patches prioritized
- Breaking changes flagged for manual review