<p align="center">
  <img src="images/fernrohr-logo.png" alt="Fernrohr" width="320">
</p>

# Fernrohr App

A Rust desktop application for Kubernetes cluster monitoring and resource browsing, built with GPUI and GPUI-Kit.

## About This Project

Fernrohr App is the desktop UI component of the Fernrohr meta-repository, providing developers with an intuitive interface to browse, inspect, and manage Kubernetes cluster resources.

## Building

### Prerequisites

- Rust 1.70+
- macOS 11+ (currently macOS-only)
- `ssh` on `PATH` - required to connect any kube context bound to an SSH tunnel; Fernrohr
  shells out to the system OpenSSH client rather than bundling its own

### Build Commands

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run in development
cargo run

# Run tests
cargo test

# Run linting
cargo clippy -- -D warnings

# Format code
cargo fmt
```

## Running the App

The macOS `.app` and `.dmg` the Package workflow builds are ad-hoc signed, so Apple Silicon runs
the app and TCC can remember its local network permission. You no longer need to sign it yourself.

An ad-hoc signature carries no Developer ID and isn't notarized, though, so Gatekeeper still blocks
a copy downloaded through a browser: it reports the app as damaged or from an unidentified
developer. Once you have the app installed in `/Applications`, clear the quarantine flag:

```sh
xattr -d com.apple.quarantine "/Applications/Fernrohr.app"
```

Or open it once from System Settings → Privacy & Security → Open Anyway. Builds made before
signing landed still need `codesign --force --sign - "/Applications/Fernrohr.app"` as well.

## Keyboard

Every action is a command in the command palette (⌘⇧P), and you can change any command's key in
Settings → Keyboard Shortcuts or in `keymap.toml` in the app's preferences folder.

Some shortcuts take two keys, such as `⌘K ←` to split a panel group. After the first key, the
status bar shows the keys so far, such as `⌘K …`, and above it the keys that would finish a
shortcut there, each with what it does. Press one of those keys to run it. Any other key, or a
change of focus, cancels the shortcut.

If the keys so far are a shortcut on their own as well as the start of a longer one, Fernrohr waits
for the next key before it runs the shorter shortcut. Set how long in Settings → Shortcut Timeout,
from 1 to 10 seconds (3 by default), or as `shortcut_timeout_secs` in `ui.toml`. A first key that
isn't a shortcut on its own, like `⌘K`, waits for its next key however long you take.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow and contribution guidelines.

## License

See [LICENSE.md](LICENSE.md) for details.

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
