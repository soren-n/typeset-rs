## [3.0.4](https://github.com/soren-n/typeset-rs/compare/v3.0.3...v3.0.4) (2025-08-17)


### Bug Fixes

* **ci:** resolve circular dependency during crate publishing ([ee10693](https://github.com/soren-n/typeset-rs/commit/ee1069383b5347edd90b16de9f923ba9ee05eb05))

## [3.0.3](https://github.com/soren-n/typeset-rs/compare/v3.0.2...v3.0.3) (2025-08-17)


### Bug Fixes

* **ci:** publish typeset-parser before typeset to resolve dependency issue ([4eaecd7](https://github.com/soren-n/typeset-rs/commit/4eaecd7202807cf5217de9ab59d94c5a4cc572b9))

## [3.0.2](https://github.com/soren-n/typeset-rs/compare/v3.0.1...v3.0.2) (2025-08-17)


### Bug Fixes

* **ci:** set GitHub Actions outputs for semantic-release job ([0bd32bd](https://github.com/soren-n/typeset-rs/commit/0bd32bdaccf62bf22798f7c29d8f5258d90506b0))

## [3.0.1](https://github.com/soren-n/typeset-rs/compare/v3.0.0...v3.0.1) (2025-08-17)


### Bug Fixes

* **release:** update version script to handle bidirectional dependencies ([7eeabca](https://github.com/soren-n/typeset-rs/commit/7eeabca81eb6456517579478c30a5f6fa9a201b6))

# [3.0.0](https://github.com/soren-n/typeset-rs/compare/v2.0.5...v3.0.0) (2025-08-17)


### Bug Fixes

* add missing CI workflow file ([6c95482](https://github.com/soren-n/typeset-rs/commit/6c95482338306ef7a556d56acb8a8f46e70ae004))
* **ci:** resolve GitHub Actions workflow failures ([942fb7c](https://github.com/soren-n/typeset-rs/commit/942fb7c73266f6c36a3f990e7c912fcc2245b50a))
* **ci:** resolve remaining workflow issues ([27a524b](https://github.com/soren-n/typeset-rs/commit/27a524bb1250a74b333bd4e1c5cd8b322ef52e44))
* **ci:** temporarily disable OCaml and security audit jobs ([afb916f](https://github.com/soren-n/typeset-rs/commit/afb916fc842ef24a4b233205125aec45c32b56c1))
* **release:** resolve semantic-release sed command syntax error ([264f080](https://github.com/soren-n/typeset-rs/commit/264f080f1c2831d580e60e6ec035d46ddb4d7952))
* resolve CI/CD workflow failures ([645022f](https://github.com/soren-n/typeset-rs/commit/645022f73f61d6e06b71ac1f21f50871a37b1b17))


### Features

* add comprehensive git pre-commit hooks ([b0e6047](https://github.com/soren-n/typeset-rs/commit/b0e6047c869ae24db2dd17265af2b208d1aaf773))
* implement comprehensive CI/CD with semantic versioning ([a729fc7](https://github.com/soren-n/typeset-rs/commit/a729fc7855f661be72069ef26ec0dd799a29fbaa))
* improve OCaml testing support in git hooks ([37e9076](https://github.com/soren-n/typeset-rs/commit/37e9076b6d0476c04252e000165a751a51686407))
* major restructure and improvements ([7ee88ea](https://github.com/soren-n/typeset-rs/commit/7ee88eac42a46b7cef9897c8364c003cf2990edc))


### BREAKING CHANGES

* CI/CD pipeline now requires conventional commit messages for releases

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive CI/CD pipeline with GitHub Actions
- Automatic semantic versioning based on conventional commits
- OCaml integration testing in CI
- Security vulnerability scanning
- Automated dependency updates
- Pre-commit git hooks for code quality
- Comprehensive documentation for contributors

### Changed
- Modernized GitHub Actions workflows
- Enhanced code quality gates
- Improved development workflow

### Fixed
- Updated deprecated GitHub Actions
- Resolved clippy warnings and formatting issues

---

*Note: This changelog is automatically maintained by semantic-release based on conventional commit messages.*
