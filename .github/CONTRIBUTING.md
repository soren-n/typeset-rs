# Contributing to typeset-rs

Thank you for your interest in contributing! This guide will help you understand our development process and automated workflows.

## Development Setup

1. **Prerequisites**
   - Rust (stable toolchain, MSRV 1.89.0)
   - OCaml and opam (for tests)
   - Git

2. **Clone and setup**
   ```bash
   git clone https://github.com/soren-n/typeset-rs.git
   cd typeset-rs
   ```

3. **Install the OCaml test dependencies**
   ```bash
   opam install qcheck typeset
   ```

4. **Install git hooks**
   Hooks are tracked in `.githooks/` but are not active in a fresh clone. Enable
   them once:
   ```bash
   ./scripts/install-hooks.sh
   ```
   The pre-commit hook then runs formatting, linting, type checking, and both
   test suites before each commit.

## Commit Message Format

We follow [Conventional Commits](https://conventionalcommits.org/) style for a
readable, greppable history. Versioning is **not** automated — commit types no
longer trigger version bumps (releases are cut by explicit tags, see
[Release Process](#release-process)) — but consistent messages are still
expected.

### Format
```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Types
- `feat`: A new feature
- `fix`: A bug fix
- `docs`: Documentation only changes
- `style`: Changes that do not affect the meaning of the code
- `refactor`: A code change that neither fixes a bug nor adds a feature
- `perf`: A code change that improves performance
- `test`: Adding missing tests or correcting existing tests
- `chore`: Changes to the build process or auxiliary tools

### Examples
```bash
feat: add new layout constructor for tables
fix: resolve memory leak in compiler
docs: update README with installation instructions
feat!: change API for layout composition
```

## CI/CD Workflows

### 1. CI Workflow (`.github/workflows/ci.yml`)
**Triggers:** Every push and PR to main
- Code formatting (`cargo fmt`)
- Linting (`cargo clippy`)
- Rust tests (`cargo test`)
- OCaml tests (property-based testing)
- Security audit (`cargo audit`, `cargo deny`)
- Build verification

### 2. Release Workflow (`.github/workflows/release.yml`)
**Triggers:** Pushing a `v*` tag
- **Version guard**: verifies the tag matches the `Cargo.toml` version, fails on mismatch
- **Build & test**: builds release and runs the full test suite
- **Crate publishing**: publishes `typeset-parser`, then `typeset`, to crates.io
- **GitHub release**: creates a release linking to `CHANGELOG.md`

Version bumping and `CHANGELOG.md` are manual (see [Release Process](#release-process)).

### 3. Dependencies Workflow (`.github/workflows/dependencies.yml`)
**Triggers:** Weekly schedule + manual dispatch
- Updates Rust dependencies
- Security vulnerability scanning
- Creates automated PRs for dependency updates

## Testing

### Local Testing
```bash
# Run all checks (same as CI)
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test --all --all-features

# Run OCaml tests
cd tests && ./build.sh && ./run.sh

# Quick formatting fix
./scripts/fix-code-quality.sh
```

### Test Structure
- **Rust tests**: Unit tests and doc tests in `cargo test`
- **OCaml tests**: Property-based tests in `tests/tester/`
- **Integration**: Both test suites verify the same functionality

## Security

- **Dependency scanning**: Automated vulnerability detection
- **License compliance**: Only approved licenses allowed
- **Supply chain**: Dependencies verified and audited

## Pull Request Process

1. **Create feature branch**: `git checkout -b feat/your-feature`
2. **Make changes** with conventional-commit-style messages
3. **Ensure tests pass**: Pre-commit hooks will verify
4. **Create PR**: CI will run full test suite
5. **Review process**: Maintainer review required
6. **Merge**: Squash and merge

## Release Process

Releases are cut explicitly by a maintainer; merging to main does not publish
anything. Both crates share one version from `[workspace.package]`.

1. **Bump the version**: `./scripts/update-version.sh 3.3.0`
2. **Update the changelog**: edit `CHANGELOG.md` for the new version
3. **Commit**: `git commit -am "chore(release): 3.3.0"`
4. **Tag & push**: `git tag v3.3.0 && git push origin main --tags`

Pushing the tag runs `release.yml`, which verifies the tag matches `Cargo.toml`,
builds, tests, publishes both crates to crates.io, and creates the GitHub
release. Pick the version number per [semver](https://semver.org/).

## Tips

- Follow conventional-commit style for a readable history
- Pre-commit hooks catch issues early
- CI runs the same Rust checks as the git hooks (formatting, clippy, type
  checking, `cargo test`), but does **not** run the OCaml property tests — those
  run locally via the pre-commit hook only
- OCaml tests provide additional validation
- Dependency updates are automated weekly
- Security scanning runs on every change

## Getting Help

- Create an issue for bugs or feature requests
- Check existing issues and PRs
- Review the documentation in README.md and CLAUDE.md
- CI logs provide detailed error information