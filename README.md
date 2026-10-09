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

## Installing on Linux

Each release has Linux packages for x86_64 and for arm64 (aarch64). Pick the one for your system:

| Package | For | Install |
|---|---|---|
| `.deb` | Debian, Ubuntu and derivatives | `sudo apt install ./fernrohr_<version>_amd64.deb` (`_arm64.deb` on arm64) |
| `.rpm` | Fedora, RHEL and derivatives | `sudo dnf install ./fernrohr-<version>-1.x86_64.rpm` (`.aarch64.rpm` on arm64) |
| AppImage | Any distribution, without installing | `chmod +x fernrohr_<version>_x86_64.AppImage`, then run it |

The `.deb` and `.rpm` bring in what Fernrohr needs at run time:

- **`openssh-client`** (`openssh-clients` on Fedora): Fernrohr runs `ssh` for SSH tunnels.
- **`libsecret`**: Fernrohr stores tunnel keys and other credentials through the Secret Service.
  A Secret Service provider has to be running for that, so the packages recommend one
  (`gnome-keyring`, KWallet or KeePassXC). Most desktops already run one, and it can be any of
  them.

The AppImage bundles `libsecret` but can't install anything, so the host needs `ssh` on its `PATH`
and a running Secret Service provider.

### Verifying a download

Each release also has a `SHA256SUMS` file. Download it into the same directory as the package,
then check the package against it:

```sh
sha256sum --check --ignore-missing SHA256SUMS
```

The package's line should end in `OK`. `--ignore-missing` skips the other packages listed in the
file that you didn't download.

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

## Shells

`s` on a running pod, in the Pods panel or a pod's detail panel, opens a shell in its container,
asking which when it runs more than one. The shell is a real terminal (a Kubernetes exec with a
TTY), so full-screen programs like `top` and `vi`, Tab completion and Ctrl-C work as they would
locally, and the shell is resized with its panel. It uses the app's code font, text size and
colours.

While the terminal has focus it takes every key except the app's `⌘` shortcuts: copy (`⌘C`) and
paste (`⌘V`) - `Ctrl-Shift-C` and `Ctrl-Shift-V` on Linux - the command palette (`⌘⇧P`), and panel
and tab navigation. Scroll back through earlier output with the mouse wheel, and select it with
the mouse to copy it. When the shell exits, the panel says how it ended, and its screen and
scrollback stay to read and copy.

## Tunnels

A kube context can be bound to a tunnel, so Fernrohr reaches its cluster through it. Manage
tunnels in the Tunnels window (Context → Manage Tunnels…). Bind a context to one with Set Tunnel
for Context, or with the tunnel selector on the context's row in the cluster picker. A tunnel is
one of three kinds.

- **SSH tunnel**: Fernrohr runs `ssh -N -L` through a bastion to the context's API server.
- **Command tunnel**: Fernrohr runs a command you give it, such as a vendor CLI that opens an SSH
  session through an identity-aware proxy, and keeps it running while a bound context is
  connected. For example:

  ```
  gcloud compute ssh <bastion-host> --tunnel-through-iap --project <project> \
      -- -N -L{port}:127.0.0.1:8888
  ```

  `{port}` becomes the local port Fernrohr picks. If the command must use a fixed port instead,
  write that port into the command and set it as the tunnel's local port. The command runs
  without a shell, so pipes and `$VARIABLES` don't apply. Fernrohr looks it up on your login
  shell's `PATH`, even when it was opened from the Dock.

  Choose what the local port offers:
  - **Proxy** (the default): an HTTP proxy, as in the example, where port 8888 on the bastion
    is a proxy. Bound contexts keep their API server address and send their traffic through it.
  - **Forward**: the API server itself. Bound contexts connect to the local port instead.

  All contexts bound to a command tunnel share one running command. Test runs the command until
  it is ready, then stops it, and shows the command's output if it fails. The command must run
  unattended, so run it once in a terminal first to answer any host-key or login prompts. It is
  stored in plain text in `tunnels.toml`: leave credentials to the tool's own login, not flags.

- **Manual tunnel**: Fernrohr starts nothing. Use it for a cluster you can only reach after
  bringing up a network path by hand, such as a VPN from a menu-bar client with no command line.
  When a context bound to it connects, Fernrohr holds that connection and asks you to bring the
  path up, then waits for your answer, with no timeout:
  - **Proceed** connects every context waiting on the tunnel.
  - **Cancel** fails them, saying you cancelled it. Fernrohr never connects them directly
    instead.

  A manual tunnel has two settings:
  - **Instruction** (optional): shown in every prompt, for example "Connect the corporate VPN
    from the menu bar".
  - **When the API server already answers**:
    - **Skip the prompt** (the default): before asking, Fernrohr tries a quick connection to
      the context's API server and goes straight through if it answers. Choose
      **Always prompt** for an API server that answers without your VPN too.

  You are asked in three places:
  - a desktop notification;
  - the waiting context's capsule in the status bar, filled in the warning color, with Proceed
    and Cancel in its menu;
  - the cluster picker's status line, while it is connecting that context.

  From the keyboard, Proceed with Manual Tunnel (⌘⌥P, Ctrl+Alt+P off macOS) and Cancel Manual
  Tunnel (⌘⌥C, Ctrl+Alt+C) answer it. Both are in the command palette and can be rebound in
  `keymap.toml`. When several tunnels are waiting, they ask which one.

  Every context bound to the same manual tunnel shares one prompt. A context that connects
  while the tunnel is already confirmed and in use goes straight through. Once the last context
  using it disconnects, or a connection through it fails, the next connection asks again. Other
  contexts keep working while one waits.

  On macOS, a development build run with `cargo run` has no app bundle, so the system may not
  show its notifications. The status bar and the picker still ask.

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for development workflow and contribution guidelines.

## License

See [LICENSE.md](LICENSE.md) for details.

## Code of Conduct

This project adheres to the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).
