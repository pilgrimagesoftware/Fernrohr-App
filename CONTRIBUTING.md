# Contributing to Fernrohr App

Thank you for your interest in contributing! This document provides guidelines for contributing to the Fernrohr App.

## Getting Started

1. Fork the repository
2. Clone your fork locally
3. Create a feature branch: `git checkout -b feature/your-feature-name`
4. Make your changes
5. Run tests and linting locally (see "Running Checks Locally" below)
6. Commit using Conventional Commits format
7. Push and open a pull request

## Committing Code

We use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages:

```
type(scope): description

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`

Example: `feat(ui): add window resizing support`

## Running Checks Locally

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run clippy (linting)
cargo clippy -- -D warnings

# Format code
cargo fmt
```

## Code Style

- Follow Rust conventions enforced by `rustfmt`
- Fix all clippy warnings before submitting
- Tests should be included with meaningful coverage
- Comments should explain the "why", not the "what"

## Branches and Workflow

- `master` - production-ready releases
- `develop` - integration branch for features
- Feature branches created from `develop`, merged back via PR with CI checks

## Pull Request Process

1. Ensure all checks pass (CI, tests, linting, formatting)
2. Fill out the PR description with context on what changed and why
3. At least one approval before merging
4. Auto-merge when green and approved

## Code of Conduct

This project adheres to the Contributor Covenant Code of Conduct. By participating, you are expected to uphold this code.
