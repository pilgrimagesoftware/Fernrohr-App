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

use super::kubeconfig::{self, ServerForContextError};
use crate::forward::registry::{ForwardRegistry, RegistryHandle};
use crate::forward::supervisor::{BackoffPolicy, SupervisorOptions};
use crate::tunnel::ssh::{SshTunnel, SshTunnelConfig, TransientIdentityFile};
use crate::tunnel::store::{TunnelStore, TunnelStoreError};
use gpui_kit::{App, Global};
use std::collections::BTreeSet;
use std::io;
use std::path::Path;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

/// Identifies one shared SSH forward: a tunnel and the API server it reaches through
/// that tunnel. Two contexts collide on this key - and so share one `ssh` process -
/// exactly when they use the same tunnel to reach the same host and port.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ForwardKey {
    pub tunnel_id: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug)]
pub enum TunnelAcquireError {
    Store(TunnelStoreError),
    Io(io::Error),
    /// The bound context's kubeconfig `server:` URL couldn't be resolved to a target -
    /// most commonly, it has no host. No `ssh` is started for this outcome.
    Target(ServerForContextError),
}

impl std::fmt::Display for TunnelAcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(err) => write!(f, "tunnel store error: {err:?}"),
            Self::Io(err) => write!(f, "failed to prepare the tunnel forward: {err}"),
            Self::Target(err) => write!(f, "{err}"),
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
    registry: ForwardRegistry<ForwardKey, SshTunnel>,
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
) -> Result<Option<RegistryHandle<ForwardKey, SshTunnel>>, TunnelAcquireError> {
    let store = TunnelStore::new(tunnels_config_path.to_path_buf());
    let Some(tunnel_id) = store.binding_for(context) else {
        return Ok(None);
    };
    let Some(config) = store.get(&tunnel_id) else {
        // A binding pointing at a deleted tunnel: treat as unbound rather than
        // failing the connection outright.
        return Ok(None);
    };
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

    let key = ForwardKey {
        tunnel_id,
        host: target_host,
        port: target_port,
    };
    let handle = registry.acquire(key, || {
        SshTunnel::spawn(&rt, ssh_config, identity_file, local_addr, options)
    });
    Ok(Some(handle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tunnels::TunnelConfig;
    use crate::forward::managed::ManagedForward as _;
    use crate::tunnel::store::TunnelStore;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_tunnels_path() -> std::path::PathBuf {
        crate::util::test_paths::temp_path("cluster-tunnel")
    }

    /// Writes a throwaway kubeconfig whose contexts map 1:1 to `(context, server)`
    /// pairs, e.g. `[("staging", "https://10.0.0.1:6443")]`.
    fn kubeconfig_fixture(contexts: &[(&str, &str)]) -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fernrohr-cluster-tunnel-kubeconfig-{}-{n}.yaml",
            std::process::id()
        ));

        let mut clusters = String::new();
        let mut ctxs = String::new();
        let mut users = String::new();
        for (name, server) in contexts {
            clusters.push_str(&format!(
                "  - name: {name}\n    cluster:\n      server: {server}\n"
            ));
            ctxs.push_str(&format!(
                "  - name: {name}\n    context:\n      cluster: {name}\n      user: {name}\n"
            ));
            users.push_str(&format!("  - name: {name}\n    user: {{}}\n"));
        }
        let yaml = format!(
            "apiVersion: v1\nkind: Config\nclusters:\n{clusters}contexts:\n{ctxs}users:\n{users}"
        );
        std::fs::write(&path, yaml).unwrap();
        path
    }

    fn sample_tunnel(name: &str) -> TunnelConfig {
        TunnelConfig {
            name: name.to_string(),
            bastion_user: "deploy".to_string(),
            bastion_host: "bastion.example.invalid".to_string(),
            bastion_port: 22,
            jump_hosts: Vec::new(),
            auth: crate::config::tunnels::TunnelAuth::default(),
            ..Default::default()
        }
    }

    #[test]
    fn store_and_io_errors_convert_into_a_tunnel_acquire_error() {
        assert!(matches!(
            TunnelAcquireError::from(TunnelStoreError::NotFound),
            TunnelAcquireError::Store(TunnelStoreError::NotFound)
        ));
        let io_err = io::Error::from(io::ErrorKind::AddrInUse);
        assert!(matches!(
            TunnelAcquireError::from(io_err),
            TunnelAcquireError::Io(_)
        ));
    }

    #[gpui_kit::test]
    async fn unbound_context_returns_none_without_touching_the_registry(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let path = temp_tunnels_path();
        cx.update(crate::runtime::init);

        let result = cx.update(|cx| acquire_for_context(cx, &path, None, "no-such-context"));
        assert!(matches!(result, Ok(None)));
        // Section 6.3: an unbound context must never initialize `TunnelForwards` -
        // `ForwardRegistry` stays untouched, not just empty.
        assert!(!cx.update(|cx| cx.has_global::<TunnelForwards>()));

        let _ = std::fs::remove_file(&path);
    }

    /// Section 1.2: a context bound to a tunnel acquires that tunnel's forward when
    /// looked up by its own name, not the kubeconfig's `current-context` - the case
    /// `connection.rs`'s `connect(cx, Some(context_name))` now relies on via
    /// `resolve_bound_context`. Section 2.3: the forward's target now comes from a
    /// fixture kubeconfig rather than a `remote_host`/`remote_port` on the tunnel
    /// itself. Only acquisition is asserted here; the forward actually reaching `Up`
    /// against a real bastion is `tunnel-bastion-verification` (deferred, blocked on a
    /// real bastion to test against).
    #[gpui_kit::test]
    async fn bound_context_acquires_its_tunnel_by_name(cx: &mut gpui_kit::TestAppContext) {
        let tunnels_path = temp_tunnels_path();
        let kubeconfig_path = kubeconfig_fixture(&[("staging", "https://10.0.0.1:6443")]);

        let store = TunnelStore::new(tunnels_path.clone());
        store
            .create("staging-bastion", sample_tunnel("staging"), None)
            .unwrap();
        store.bind("staging", "staging-bastion").unwrap();

        cx.update(crate::runtime::init);
        let result = cx
            .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "staging"));
        assert!(matches!(result, Ok(Some(_))));
        // A context sharing the kubeconfig's current-context, but not named "staging",
        // must not match - the binding is looked up by the exact name passed in.
        let miss = cx.update(|cx| {
            acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "not-staging")
        });
        assert!(matches!(miss, Ok(None)));

        let _ = std::fs::remove_file(&tunnels_path);
        let _ = std::fs::remove_file(&kubeconfig_path);
    }

    /// Tasks.md 2.3: a bound context whose kubeconfig server URL has no host fails the
    /// acquire with that error, and never touches `TunnelForwards` - no `ssh` starts.
    #[gpui_kit::test]
    async fn hostless_server_fails_the_acquire_and_starts_no_ssh(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let tunnels_path = temp_tunnels_path();
        let kubeconfig_path = kubeconfig_fixture(&[("broken", "https://")]);

        let store = TunnelStore::new(tunnels_path.clone());
        store
            .create("some-bastion", sample_tunnel("some"), None)
            .unwrap();
        store.bind("broken", "some-bastion").unwrap();

        cx.update(crate::runtime::init);
        let result = cx
            .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "broken"));
        assert!(matches!(
            result,
            Err(TunnelAcquireError::Target(ServerForContextError::NoHost(_)))
        ));
        assert!(
            !cx.update(|cx| cx.has_global::<TunnelForwards>()),
            "a target-resolution failure must start no ssh"
        );

        let _ = std::fs::remove_file(&tunnels_path);
        let _ = std::fs::remove_file(&kubeconfig_path);
    }

    /// Tasks.md 2.3: two contexts sharing one tunnel but pointing at different API
    /// servers each get their own forward - `ForwardKey` includes the target, not just
    /// the tunnel id, so they don't collide the way two contexts on the *same* server
    /// through the same tunnel would.
    #[gpui_kit::test]
    async fn two_contexts_on_different_servers_through_one_tunnel_get_separate_forwards(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let tunnels_path = temp_tunnels_path();
        let kubeconfig_path = kubeconfig_fixture(&[
            ("cluster-a", "https://10.0.0.1:6443"),
            ("cluster-b", "https://10.0.0.2:6443"),
        ]);

        let store = TunnelStore::new(tunnels_path.clone());
        store
            .create("shared-bastion", sample_tunnel("shared"), None)
            .unwrap();
        store.bind("cluster-a", "shared-bastion").unwrap();
        store.bind("cluster-b", "shared-bastion").unwrap();

        cx.update(crate::runtime::init);
        let a = cx
            .update(|cx| {
                acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "cluster-a")
            })
            .unwrap()
            .expect("cluster-a is bound");
        let b = cx
            .update(|cx| {
                acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "cluster-b")
            })
            .unwrap()
            .expect("cluster-b is bound");

        assert_ne!(
            a.forward().local_addr(),
            b.forward().local_addr(),
            "different API servers through one tunnel must produce two forwards"
        );

        let _ = std::fs::remove_file(&tunnels_path);
        let _ = std::fs::remove_file(&kubeconfig_path);
    }

    /// Tasks.md 3.3: rebinding a context mid-connection never alters the connection
    /// already using its old tunnel - `connect` (and so `acquire_for_context`) only
    /// ever runs at connection start, so the already-acquired `RegistryHandle` simply
    /// keeps pointing at its own forward regardless of what `tunnels.toml` says
    /// afterward. The *next* acquire, though, picks up the new binding.
    #[gpui_kit::test]
    async fn rebinding_a_context_never_alters_its_live_connection(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let tunnels_path = temp_tunnels_path();
        let kubeconfig_path = kubeconfig_fixture(&[("staging", "https://10.0.0.1:6443")]);

        let store = TunnelStore::new(tunnels_path.clone());
        store.create("bastion-a", sample_tunnel("a"), None).unwrap();
        store.create("bastion-b", sample_tunnel("b"), None).unwrap();
        store.bind("staging", "bastion-a").unwrap();

        cx.update(crate::runtime::init);
        let handle_a = cx
            .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "staging"))
            .unwrap()
            .expect("staging is bound to bastion-a");
        let addr_a = handle_a.forward().local_addr();

        // Rebind mid-connection: nothing about the live handle changes.
        store.bind("staging", "bastion-b").unwrap();
        assert_eq!(
            handle_a.forward().local_addr(),
            addr_a,
            "the live connection must keep using bastion-a's forward after a rebind"
        );

        // Reconnecting - a fresh acquire - picks up the new binding and gets its own
        // forward, distinct from the still-live handle_a.
        let handle_b = cx
            .update(|cx| acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "staging"))
            .unwrap()
            .expect("staging is now bound to bastion-b");
        assert_ne!(
            handle_b.forward().local_addr(),
            addr_a,
            "the next connection must acquire bastion-b's own forward"
        );

        drop(handle_a);
        drop(handle_b);
        let _ = std::fs::remove_file(&tunnels_path);
        let _ = std::fs::remove_file(&kubeconfig_path);
    }

    /// Section 4.1's "running state follows a fake forward's acquire and release":
    /// a hand-driven `watch::Sender<BTreeSet<ForwardKey>>` stands in for
    /// `ForwardRegistry::live_keys` (which only ever changes on a real acquire/release,
    /// per `forward/registry.rs`'s own tests), and `drive_live_keys` must report the
    /// initial snapshot immediately, then each change, in order.
    #[tokio::test]
    async fn drive_live_keys_reports_the_initial_set_then_each_change() {
        let key = ForwardKey {
            tunnel_id: "qa-bastion".to_string(),
            host: "10.0.0.1".to_string(),
            port: 6443,
        };
        let (state_tx, state_rx) = watch::channel(BTreeSet::new());
        let (tx, mut rx) = mpsc::channel(4);
        let handle = tokio::spawn(drive_live_keys(state_rx, tx));

        assert_eq!(rx.recv().await, Some(BTreeSet::new()));

        // "acquire"
        state_tx
            .send(BTreeSet::from([key.clone()]))
            .expect("receiver still live");
        assert_eq!(rx.recv().await, Some(BTreeSet::from([key.clone()])));

        // "release"
        state_tx.send(BTreeSet::new()).expect("receiver still live");
        assert_eq!(rx.recv().await, Some(BTreeSet::new()));

        drop(state_tx);
        handle.await.unwrap();
        assert_eq!(rx.recv().await, None);
    }
}
