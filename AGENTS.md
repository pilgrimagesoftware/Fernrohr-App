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

## Keyboard Navigation Is First-Class

Fernrohr is keyboard-first. Every feature must be fully usable from both the keyboard and the
mouse. A feature that only one of them can reach isn't done.

- Every action (select, open, connect, edit, delete, confirm, cancel) has a keyboard route and a
  mouse route.
- User-facing actions go through the command registry (`src/command.rs`: id, title, default binding,
  `KeyContext`, menu slot), so each one gets a keybinding, a palette entry and a menu item.
- One selection, moved by both clicks and keyboard navigation; hover never moves it. gpui-component's
  `Command` selects on hover, so use `window.last_input_was_keyboard()` to tell keyboard input from
  hover (see `ClusterPicker::follow_keyboard` in `src/ui/picker.rs`).
- Show shortcuts in a hint row from the live keymap (`Kbd::binding_for_action`), and make dialogs
  work from the keyboard: Tab, Enter or Space, Escape, and sensible focus.
- Tests cover the keyboard route with real keystrokes (`VisualTestContext::simulate_keystrokes`),
  not only direct handler calls.

The full rule, which the meta repo loads for agents automatically, is
`.claude/rules/keyboard-first.md` in the parent Fernrohr repo.

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
cargo test -- --ignored       # Also run the real-keychain tests (expect a macOS
                               # Keychain access prompt on each freshly built binary)
```

A plain `cargo test` never touches the real OS keychain: tests that exercise it for
real (`tunnel::secrets`, `util::keychain`) are `#[ignore]`d, since every freshly built
test binary triggers a macOS Keychain permission prompt on first access. Everything
else that only uses a secret store incidentally (`TunnelStore` CRUD/bind, the tunnel
editor, the connect path) gets an in-memory-only secret store under `cfg(test)`
instead, so it never reaches the keychain at all.

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
