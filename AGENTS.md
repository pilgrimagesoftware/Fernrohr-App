# AGENTS.md

This file provides guidance to Claude Code and other AI agents when working with code in this repository.

## About This Project

Fernrohr App is the desktop UI component for Kubernetes cluster monitoring, built as a Rust application using GPUI and GPUI-Kit. It is part of the larger Fernrohr meta-repository project.

### Dependencies

This repo is a component of the Fernrohr meta-repository:
- Parent repo: [Fernrohr](https://github.com/paulyhedral/Fernrohr)
- Sibling component: Fernrohr platform/backend services

## Committing Code

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`

Example: `feat(ui): add pod details panel`

Commits are automatically signed. Never skip hooks with `--no-verify`.

## Branches and Workflow

- `master` - stable releases only
- `develop` - integration branch for features
- Feature branches created from `develop` with naming: `feature/description` or `fix/description`
- PRs require CI to pass and at least one approval
- Auto-merge enabled when all checks pass

## Running Checks Locally

### Build
```bash
cargo build                    # Debug build
cargo build --release         # Optimized release build
cargo run                      # Run application
```

### Testing
```bash
cargo test                     # Run all tests
cargo test --lib             # Run library tests only
```

### Linting and Formatting
```bash
cargo clippy -- -D warnings   # Run clippy (enforces all warnings as errors)
cargo fmt                      # Format code
cargo fmt -- --check          # Check formatting without changes
```

### Full Pre-Commit Check
```bash
cargo fmt && cargo clippy -- -D warnings && cargo test
```

## File Organization

- `src/` - Rust source code
  - `main.rs` - Application entry point and main window
  - Additional modules for UI components and logic
- `Cargo.toml` - Project manifest and dependencies

## Dependencies

Key dependencies:
- **gpui** - Immediate mode UI framework
- **gpui-component** - High-level UI components
- Other Rust standard ecosystem libraries

Keep dependencies up-to-date; Dependabot opens PRs weekly.

## Platform Conventions and Decisions

This repo is a submodule of the Fernrohr platform. Platform-wide conventions and Architecture Decision Records live there:

- checked out inside the platform tree: `../../docs/README.md` (convention index) and `../../docs/adr/README.md` (platform ADRs, `PADR-*`)
- standalone or module-only checkout: <https://github.com/paulyhedral/Fernrohr/tree/master/docs> and <https://github.com/paulyhedral/Fernrohr/tree/master/docs/adr>

Accepted `PADR-*` records are binding constraints. Read the ADR index before proposing a structural change; if a task needs to contradict an accepted record, stop and propose a superseding ADR rather than working around it. This repo's own service-local decisions are `ADR-*` in `docs/adr/` here. `/adr "<title>"` scaffolds one.
