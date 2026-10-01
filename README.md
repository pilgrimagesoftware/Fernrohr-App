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

The app likely needs to local network permissions, and since it's unsigned, TCC on the Mac will
not allow it to make network connections. Once you have the app installed in `/Applications`, run
the following commands:

```sh
# remove quarantine
xattr -d com.apple.quarantine "/Applications/Fernrohr.app"
# ad-hoc sign
codesign --force --deep --sign - "/Applications/Fernrohr.app"
```

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow and contribution guidelines.

## License

See [LICENSE.md](LICENSE.md) for details.

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
