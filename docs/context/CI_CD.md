# CI/CD Pipeline

## Overview

The project uses automated GitHub Actions workflows for continuous integration, releases, and dependency management, with semantic versioning based on conventional commits.

## Workflows

### 1. CI Pipeline (`.github/workflows/ci.yml`)
**Triggers**: Every push and pull request

**Quality Gates**:
- **Formatting**: `cargo fmt --check` (must pass)
- **Linting**: `cargo clippy` (warnings allowed, errors blocked)  
- **Type Checking**: `cargo check --all-targets --all-features`
- **Testing**: Both Rust (`cargo test`) and OCaml property-based tests

**Matrix Testing**:
- Rust stable and nightly versions
- Multiple OS environments (if configured)

**Security & Compliance**:
- `cargo-audit`: Security vulnerability scanning
- `cargo-deny`: License and dependency policy enforcement
- Build verification and artifact generation

### 2. Release Pipeline (`.github/workflows/release.yml`)
**Triggers**: Merges to main branch (when version bumps are needed)

**Automated Semantic Versioning**:
- Analyzes conventional commit messages
- Determines appropriate version bump (major/minor/patch)
- Updates `Cargo.toml` versions automatically
- Generates `CHANGELOG.md` from commit messages

**Release Process**:
- Creates git tags with new versions
- Generates GitHub releases with release notes  
- Publishes crates to crates.io in dependency order (typeset first, then typeset-parser)

### 3. Dependencies Workflow (`.github/workflows/dependencies.yml`)
**Triggers**: Weekly schedule

**Automated Maintenance**:
- Updates Rust dependencies with automated PRs
- Security vulnerability scanning
- License compliance checking
- Dependency freshness monitoring

## Semantic Versioning

### Conventional Commit Format
The project uses [Conventional Commits](https://conventionalcommits.org/) specification:

**Commit Types**:
- `feat:` → **minor** version bump (new functionality)
- `fix:` → **patch** version bump (bug fixes)
- `docs:` → patch version bump (documentation changes)
- `style:` → patch version bump (formatting, no logic changes)  
- `refactor:` → patch version bump (code restructuring)
- `test:` → patch version bump (test additions/changes)
- `chore:` → patch version bump (build changes, etc.)

**Breaking Changes**:
- `BREAKING CHANGE:` in commit body → **major** version bump
- `!` after type (e.g., `feat!:`) → **major** version bump

### Example Commit Messages
```
feat: add support for custom indentation strategies

fix: resolve memory leak in layout compilation

feat!: change compile() function signature

BREAKING CHANGE: The compile function now requires a BumpAllocator parameter
```

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

5. **Merge**: Merge to main triggers release pipeline (if applicable)
   - Automatic version analysis
   - Changelog generation  
   - Crate publishing (if version changed)

### Git Hooks (Local Development)
Pre-commit hooks enforce quality before CI:
- Code formatting check
- Clippy linting  
- Type checking
- Complete test suite (including OCaml tests)
- Auto-installs missing OCaml dependencies

**Quick fixes**:
```bash
./scripts/fix-code-quality.sh    # Auto-fix formatting and clippy issues
```

## Release Management

### Automatic Publishing
- **Workspace Dependency Order**: typeset published first, then typeset-parser
- **Version Synchronization**: Both crates updated simultaneously
- **Failure Handling**: Publication failures are logged and can be retried manually

### Manual Override
If needed, releases can be triggered manually:
```bash
# Tag manually (triggers release workflow)
git tag v0.2.0
git push origin v0.2.0
```

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