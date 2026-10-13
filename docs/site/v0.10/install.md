---
title: Install Fernrohr
version: v0.10
---

# Install Fernrohr

Each release publishes a `.dmg` for macOS on Apple silicon, and a `.deb`, an `.rpm` and an
AppImage for Linux on x86_64 and aarch64. Every file name includes the version and the
architecture. Download the one for your system from the release page.

## Install on macOS

1. Open the `.dmg` and drag Fernrohr into `/Applications`.
2. Open Fernrohr from `/Applications`.

The `.dmg` is for Apple silicon only.

The app is ad-hoc signed, which lets Apple silicon run it, but it carries no Developer ID and
isn't notarized. A copy downloaded through a browser is quarantined, so Gatekeeper reports it as
damaged or from an unidentified developer.

> Gatekeeper blocks the first launch of a downloaded copy.
{: .warning}

Use either fix once the app is in `/Applications`:

- **Terminal:** clear the quarantine flag.

  ```sh
  xattr -d com.apple.quarantine "/Applications/Fernrohr.app"
  ```

- **System Settings:** try to open the app once, then open System Settings, go to
  Privacy & Security, and choose Open Anyway next to the Fernrohr message.

Builds made before signing was added also need to be signed once by hand:

```sh
codesign --force --sign - "/Applications/Fernrohr.app"
```

### Keychain access

Fernrohr keeps tunnel secrets, such as passwords and key passphrases, in the macOS Keychain.
macOS can ask you to allow access the first time it reads them. Allow it so the secret is
remembered between sessions. If you decline, or no Keychain is available, Fernrohr asks for the
secret again for the current session instead of failing.

## Install on Linux

Pick the package for your architecture, x86_64 or arm64 (aarch64).

| Package | For | Install |
|---|---|---|
| `.deb` | Debian, Ubuntu and derivatives | `sudo apt install ./fernrohr_<version>_amd64.deb` (`_arm64.deb` on arm64) |
| `.rpm` | Fedora, RHEL and derivatives | `sudo dnf install ./fernrohr-<version>-1.x86_64.rpm` (`.aarch64.rpm` on arm64) |
| AppImage | Any distribution, without installing | `chmod +x fernrohr_<version>_x86_64.AppImage`, then run it |

### What the packages need

The `.deb` and `.rpm` pull in what Fernrohr needs at run time:

- **`openssh-client`** (`openssh-clients` on Fedora): Fernrohr runs `ssh` for SSH tunnels.
- **`libsecret`** (`libsecret-1-0` on Debian and Ubuntu): Fernrohr stores tunnel keys and other
  credentials through the Secret Service.

A Secret Service provider has to be running to hold those credentials. The packages recommend one,
without requiring it: `gnome-keyring`, KWallet or KeePassXC on Debian and Ubuntu, `gnome-keyring`
on Fedora. Most desktops already run one, and any of them works, so installation succeeds without
installing another.

Without a running provider, Fernrohr can't keep tunnel secrets between sessions. It asks for them
again each session instead. If you don't want that, install and unlock a provider such as GNOME
Keyring, KWallet or KeePassXC.

### Run the AppImage

The AppImage bundles `libsecret` but can't install anything else. The host needs two things:

- `ssh` on your `PATH`. Install `openssh-client` or `openssh-clients`.
- A running Secret Service provider, such as GNOME Keyring, KWallet or KeePassXC.

## Verify a download

Each release has a `SHA256SUMS` file with one line for each `.dmg`, `.deb`, `.rpm` and AppImage.
Download it into the same directory as your file, then check it.

On Linux:

```sh
sha256sum --check --ignore-missing SHA256SUMS
```

On macOS, which has `shasum` rather than `sha256sum`, check just the file you downloaded:

```sh
grep "<file name>" SHA256SUMS | shasum -a 256 --check
```

Your file's line should end in `OK`. On Linux, `--ignore-missing` skips the other packages listed
in `SHA256SUMS` that you didn't download.

## Where Fernrohr finds your clusters

Fernrohr reads contexts from the kubeconfig named by `$KUBECONFIG`, or from `~/.kube/config` when
that variable is unset. It never writes to the file. If there is no kubeconfig, the app opens and
reports that no contexts were found. To connect, see
[Connect to clusters]({{ '/v0.10/clusters/' | relative_url }}).

*Summarizes the capability specs: `release-packaging`, `cluster-connection`.*
