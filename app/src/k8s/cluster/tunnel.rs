//! Section 6.2 of the tunnel-subsystem change: resolves a context's tunnel binding and
//! hands `connection.rs` a shared, supervised `ssh` forward to route through.
//!
//! Owns the app-scoped [`ForwardRegistry`], keyed by [`ForwardKey`] so two contexts
//! bound to the same tunnel *and* pointed at the same API server share one `ssh`
//! process, while two contexts through the same tunnel but different API servers each
//! get their own (this is the registry's whole point - see `forward_registry.rs`).
//! Building a [`SshTunnelConfig`] from a stored [`TunnelConfig`] and allocating the
//! local port both live here too, since nothing else needs them.
//!
//! Section 2.3 of `tunnel-management-ui` moved the forward target out of
//! `tunnels.toml` and into the connecting context's own kubeconfig `server:` URL
//! (`crate::k8s::cluster::kubeconfig::server_for_context`), so `ForwardKey` carries the
//! resolved host/port alongside the tunnel id, and a context whose server URL has no
//! host now fails the acquire outright rather than falling back to a direct connect.
//!
//! `command-tunnels` adds a second kind: a command tunnel runs the user's command
//! instead of `ssh`, takes no target from the context, and is shared by every context
//! bound to it - so [`ForwardKey`] is per kind, and the registry holds a
//! [`TunnelForward`] that is either.

use super::kubeconfig::{self, ServerForContextError};
use crate::config::tunnels::{TunnelConfig, TunnelKind};
use crate::forward::managed::{ForwardState, ManagedForward as _};
use crate::forward::registry::{ForwardRegistry, RegistryHandle};
use crate::forward::supervisor::{BackoffPolicy, SupervisorOptions};
use crate::tunnel::command::{CommandTunnel, argv};
use crate::tunnel::manual::{ManualConfirmations, ManualTransport, ManualTunnel, ProbeTarget};
use crate::tunnel::ssh::{SshTunnel, SshTunnelConfig, TransientIdentityFile};
use crate::tunnel::store::{TunnelStore, TunnelStoreError};
use gpui_kit::{App, Global};
use std::collections::BTreeSet;
use std::io;
use std::path::Path;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

mod forward;
pub use forward::{TunnelForward, TunnelRoute};

/// Identifies one shared forward. Two contexts collide on a key - and so share one
/// process - when they use the same SSH tunnel to reach the same host and port, or
/// the same command tunnel, whatever their API servers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ForwardKey {
    Ssh {
        tunnel_id: String,
        host: String,
        port: u16,
    },
    Command {
        tunnel_id: String,
    },
    /// A manual tunnel: the tunnel alone, so every bound context shares one
    /// confirmation (`manual-confirmation-tunnels` D2).
    Manual {
        tunnel_id: String,
    },
}

impl ForwardKey {
    pub fn tunnel_id(&self) -> &str {
        match self {
            Self::Ssh { tunnel_id, .. }
            | Self::Command { tunnel_id }
            | Self::Manual { tunnel_id } => tunnel_id,
        }
    }
}

#[derive(Debug)]
pub enum TunnelAcquireError {
    Store(TunnelStoreError),
    Io(io::Error),
    /// The bound context's kubeconfig `server:` URL couldn't be resolved to a target -
    /// most commonly, it has no host. No `ssh` is started for this outcome.
    Target(ServerForContextError),
    /// A command tunnel's command line no longer splits - edited by hand into an
    /// unclosed quote, say. Nothing is started.
    Command(String),
}

impl std::fmt::Display for TunnelAcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(err) => write!(f, "tunnel store error: {err:?}"),
            Self::Io(err) => write!(f, "failed to prepare the tunnel forward: {err}"),
            Self::Target(err) => write!(f, "{err}"),
            Self::Command(reason) => write!(f, "{reason}"),
        }
    }
}

impl From<TunnelStoreError> for TunnelAcquireError {
    fn from(err: TunnelStoreError) -> Self {
        Self::Store(err)
    }
}

impl From<io::Error> for TunnelAcquireError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<ServerForContextError> for TunnelAcquireError {
    fn from(err: ServerForContextError) -> Self {
        Self::Target(err)
    }
}

/// How often an established forward's `ssh` child is checked for liveness.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
/// Reconnect backoff: starts at 1s, caps at 30s - see `BackoffPolicy::delay`.
const RECONNECT_BACKOFF: BackoffPolicy = BackoffPolicy {
    initial: Duration::from_secs(1),
    max: Duration::from_secs(30),
};

struct TunnelForwards {
    registry: ForwardRegistry<ForwardKey, TunnelForward>,
}

impl Global for TunnelForwards {}

impl TunnelForwards {
    fn ensure_init(cx: &mut App) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self {
                registry: ForwardRegistry::new(),
            });
        }
    }
}

/// The live set of [`ForwardKey`]s currently acquired, for a Tunnels UI to derive each
/// tunnel's running state (group by [`ForwardKey::tunnel_id`]) without polling. Safe to
/// call before any tunnel has ever been acquired - initializes an empty registry rather
/// than requiring one to already exist.
pub fn live_forward_keys(cx: &mut App) -> watch::Receiver<BTreeSet<ForwardKey>> {
    TunnelForwards::ensure_init(cx);
    cx.global::<TunnelForwards>().registry.live_keys()
}

/// Relays [`live_forward_keys`]'s watch channel onto an `mpsc` sender, reporting the
/// current set immediately and then every change - the same
/// `spawn_stream`/`runtime::drain` bridge `k8s::cluster::health::drive` uses for
/// `ForwardState`, so `ui::tunnels::list::TunnelsWindow` can derive running state from
/// an ordinary GPUI background task instead of polling. Pure and `App`-free so it's
/// testable against a hand-driven `watch::Sender` standing in for a fake forward's
/// acquire/release, the same way `health::drive`'s own tests stand in for a fake
/// `ForwardState` transition.
pub async fn drive_live_keys(
    mut rx: watch::Receiver<BTreeSet<ForwardKey>>,
    tx: mpsc::Sender<BTreeSet<ForwardKey>>,
) {
    // Bound to a local and dropped before the `.await` below rather than cloned
    // inline (`tx.send(rx.borrow_and_update().clone()).await`) - inline, the
    // borrowed `watch::Ref`'s temporary lives to the end of that statement, which
    // includes the `.await`, and it is not `Send`.
    let initial = rx.borrow_and_update().clone();
    if tx.send(initial).await.is_err() {
        return;
    }
    while rx.changed().await.is_ok() {
        let next = rx.borrow_and_update().clone();
        if tx.send(next).await.is_err() {
            return;
        }
    }
}

/// Resolves `context`'s tunnel binding (if any) via `tunnels.toml` and acquires the
/// shared forward for it, spawning a fresh `ssh` supervisor on the first acquire for
/// that (tunnel, target) key. Returns `Ok(None)` for an unbound context - section 6.3's
/// direct-connect regression path. `kubeconfig_path` resolves exactly like
/// [`kubeconfig::server_for_context`]: the given path, or `$KUBECONFIG`/
/// `~/.kube/config` when `None`.
pub fn acquire_for_context(
    cx: &mut App,
    tunnels_config_path: &Path,
    kubeconfig_path: Option<&Path>,
    context: &str,
) -> Result<Option<RegistryHandle<ForwardKey, TunnelForward>>, TunnelAcquireError> {
    let store = TunnelStore::new(tunnels_config_path.to_path_buf());
    let Some(tunnel_id) = store.binding_for(context) else {
        return Ok(None);
    };
    let Some(config) = store.get(&tunnel_id) else {
        // A binding pointing at a deleted tunnel: treat as unbound rather than
        // failing the connection outright.
        return Ok(None);
    };
    match config.kind {
        TunnelKind::Command => return acquire_command(cx, tunnel_id, &config).map(Some),
        TunnelKind::Manual => {
            return Ok(Some(acquire_manual(
                cx,
                tunnel_id,
                &config,
                kubeconfig_path,
                context,
            )));
        }
        TunnelKind::Ssh => {}
    }
    let secret = store.secret(&tunnel_id)?;

    // Resolved before `TunnelForwards::ensure_init` so an unresolvable target (no host
    // in the kubeconfig server URL) fails here, before the registry global even exists
    // and long before any `ssh` would spawn.
    let (target_host, target_port) = kubeconfig::server_for_context(kubeconfig_path, context)?;

    TunnelForwards::ensure_init(cx);
    let rt = crate::runtime::handle(cx);
    let registry = &cx.global::<TunnelForwards>().registry;

    // The registry's factory closure (below) is infallible, so every fallible step -
    // allocating the local port, writing the transient identity file - happens here,
    // before `acquire` is called, and its result is moved into the closure. `acquire`
    // doesn't expose an "already live" peek, so this work happens even when a second
    // context shares an already-running tunnel and the factory never runs; the
    // resulting `AllocatedPort`/`TransientIdentityFile` are simply dropped unused in
    // that case (the identity file's `Drop` removes it immediately).
    // ponytail: a secret briefly touches disk on every acquire, not just the first;
    // add a `ForwardRegistry::contains` peek if that overhead/exposure ever matters.
    let local_addr = crate::util::port_allocator::allocate()?.addr();
    let identity_file = secret
        .as_deref()
        .map(|secret| TransientIdentityFile::write(&tunnel_id, secret))
        .transpose()?;
    let mut ssh_config = SshTunnelConfig {
        bastion_user: config.bastion_user.clone(),
        bastion_host: config.bastion_host.clone(),
        bastion_port: config.bastion_port,
        jump_hosts: config.jump_hosts.clone(),
        remote_host: target_host.clone(),
        remote_port: target_port,
        local_port: local_addr.port(),
        identity_file: None,
        known_hosts_file: None,
        ssh_config_file: None,
    };
    if let Some(identity_file) = &identity_file {
        ssh_config.identity_file = Some(identity_file.path());
    }
    let options = SupervisorOptions {
        health_check_interval: HEALTH_CHECK_INTERVAL,
        backoff: RECONNECT_BACKOFF,
    };

    let key = ForwardKey::Ssh {
        tunnel_id,
        host: target_host,
        port: target_port,
    };
    let handle = registry.acquire(key, || {
        TunnelForward::Ssh(SshTunnel::spawn(
            &rt,
            ssh_config,
            identity_file,
            local_addr,
            options,
        ))
    });
    Ok(Some(handle))
}

/// A command tunnel's shared forward: keyed by the tunnel alone, so every bound
/// context shares one running command, and with no target taken from the context.
/// Its local port is the fixed one when set, otherwise one allocated now (as for SSH,
/// allocated even when the forward is already running and the factory never runs).
fn acquire_command(
    cx: &mut App,
    tunnel_id: String,
    config: &TunnelConfig,
) -> Result<RegistryHandle<ForwardKey, TunnelForward>, TunnelAcquireError> {
    let command = &config.command;
    let args = argv::split(&command.command_line).map_err(|error| {
        TunnelAcquireError::Command(format!(
            "the command of tunnel {:?} can't be run: {error:?}",
            config.name
        ))
    })?;
    let local_addr = match command.local_port {
        Some(port) => std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        None => crate::util::port_allocator::allocate()?.addr(),
    };
    let args = argv::substitute_port(&args, local_addr.port());
    let startup_timeout = Duration::from_secs(command.startup_timeout_secs);
    let mode = command.mode;
    let options = SupervisorOptions {
        health_check_interval: HEALTH_CHECK_INTERVAL,
        backoff: RECONNECT_BACKOFF,
    };

    TunnelForwards::ensure_init(cx);
    let rt = crate::runtime::handle(cx);
    let registry = &cx.global::<TunnelForwards>().registry;
    Ok(registry.acquire(ForwardKey::Command { tunnel_id }, || {
        TunnelForward::Command(CommandTunnel::spawn(
            &rt,
            args,
            local_addr,
            startup_timeout,
            mode,
            options,
        ))
    }))
}

/// A manual tunnel's shared forward: keyed by the tunnel alone, so every bound
/// context waits on one confirmation. With the reachability shortcut on, the first
/// acquiring context's API server is what is probed (design.md D5); a server that
/// doesn't resolve just means asking. A context acquiring while the tunnel is
/// unconfirmed is recorded as waiting on it.
fn acquire_manual(
    cx: &mut App,
    tunnel_id: String,
    config: &TunnelConfig,
    kubeconfig_path: Option<&Path>,
    context: &str,
) -> RegistryHandle<ForwardKey, TunnelForward> {
    let probe = config
        .manual
        .skip_when_reachable
        .then(|| kubeconfig::server_for_context(kubeconfig_path, context).ok())
        .flatten()
        .map(|(host, port)| ProbeTarget { host, port });
    let events = ManualConfirmations::events(cx);
    let transport = ManualTransport::new(
        tunnel_id.clone(),
        config.name.clone(),
        config.manual.message.clone(),
        probe,
        events,
    );
    let options = SupervisorOptions {
        health_check_interval: HEALTH_CHECK_INTERVAL,
        backoff: RECONNECT_BACKOFF,
    };

    TunnelForwards::ensure_init(cx);
    let rt = crate::runtime::handle(cx);
    let handle = cx.global::<TunnelForwards>().registry.acquire(
        ForwardKey::Manual {
            tunnel_id: tunnel_id.clone(),
        },
        || TunnelForward::Manual(ManualTunnel::spawn(&rt, transport, options)),
    );
    if *handle.forward().state().borrow() != ForwardState::Up {
        ManualConfirmations::note_waiting(cx, &tunnel_id, context);
    }
    handle
}

#[cfg(test)]
mod manual_tests;
#[cfg(test)]
mod tests;
