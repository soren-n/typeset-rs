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

3. **Install git hooks**
   Git hooks are automatically installed and will run quality checks before commits.

## Commit Message Format

We use [Conventional Commits](https://conventionalcommits.org/) for automatic semantic versioning and changelog generation.

### Format
```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Types
- `feat`: A new feature (triggers minor version bump)
- `fix`: A bug fix (triggers patch version bump)
- `docs`: Documentation only changes
- `style`: Changes that do not affect the meaning of the code
- `refactor`: A code change that neither fixes a bug nor adds a feature
- `perf`: A code change that improves performance
- `test`: Adding missing tests or correcting existing tests
- `chore`: Changes to the build process or auxiliary tools

### Breaking Changes
Add `!` after the type/scope or include `BREAKING CHANGE:` in the footer to trigger a major version bump.

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
**Triggers:** Push to main with releasable commits
- **Semantic versioning**: Automatically determines version based on commit messages
- **Changelog generation**: Creates/updates CHANGELOG.md
- **Version bumping**: Updates Cargo.toml files automatically
- **Crate publishing**: Publishes to crates.io in correct order
- **GitHub releases**: Creates release with generated notes

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
2. **Make changes** with conventional commit messages
3. **Ensure tests pass**: Pre-commit hooks will verify
4. **Create PR**: CI will run full test suite
5. **Review process**: Maintainer review required
6. **Merge**: Squash and merge with conventional commit message

## Release Process (Automated)

Releases are **fully automated** based on commit messages:

1. **Push to main**: Any commit with `feat:`, `fix:`, or `BREAKING CHANGE:`
2. **Version calculation**: Semantic-release analyzes commits
3. **Version update**: Cargo.toml files updated automatically
4. **Changelog**: Generated from commit messages
5. **Git tag**: Created with new version
6. **Crate publishing**: Both crates published to crates.io
7. **GitHub release**: Created with changelog

### Manual Release Override
To trigger a release manually:
```bash
git commit --allow-empty -m "feat: trigger release"
git push origin main
```

## Tips

- Use conventional commits for automatic versioning
- Pre-commit hooks catch issues early
- CI runs the same checks as git hooks
- OCaml tests provide additional validation
- Dependency updates are automated weekly
- Security scanning runs on every change

## Getting Help

- Create an issue for bugs or feature requests
- Check existing issues and PRs
- Review the documentation in README.md and CLAUDE.md
- CI logs provide detailed error information