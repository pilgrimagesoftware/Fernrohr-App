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

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow and contribution guidelines.

## License

See [LICENSE.md](LICENSE.md) for details.

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
