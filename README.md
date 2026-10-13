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
- macOS 11+ or Linux
- `ssh` on `PATH` - required to connect any kube context bound to an SSH tunnel; Fernrohr
  shells out to the system OpenSSH client rather than bundling its own
- For a command tunnel, the tool it runs (for example `gcloud`) on your login shell's `PATH`

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

## Documentation

User documentation lives at <https://pilgrimagesoftware.github.io/Fernrohr-App/>:

- [Install Fernrohr](https://pilgrimagesoftware.github.io/Fernrohr-App/latest/install/) on macOS or
  Linux, including the macOS quarantine workaround and the Linux runtime dependencies
- [Keybindings and commands](https://pilgrimagesoftware.github.io/Fernrohr-App/latest/reference/keybindings/)
- [Logs and shells](https://pilgrimagesoftware.github.io/Fernrohr-App/latest/logs-shells/)
- [Tunnels and port forwards](https://pilgrimagesoftware.github.io/Fernrohr-App/latest/tunnels-forwards/)
- [Work with coding agents](https://pilgrimagesoftware.github.io/Fernrohr-App/latest/agents/) and the
  [MCP tools reference](https://pilgrimagesoftware.github.io/Fernrohr-App/latest/reference/mcp-tools/)

The site's source is in [`docs/site`](docs/site), and [`docs/site/AGENTS.md`](docs/site/AGENTS.md)
has the authoring rules.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow and contribution guidelines.

## License

See [LICENSE.md](LICENSE.md) for details.

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
