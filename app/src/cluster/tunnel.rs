//! Section 6.2 of the tunnel-subsystem change: resolves a context's tunnel binding and
//! hands `connection.rs` a shared, supervised `ssh` forward to route through.
//!
//! Owns the app-scoped [`ForwardRegistry`], keyed by tunnel id so two contexts bound to
//! the same tunnel share one `ssh` process (this is the registry's whole point - see
//! `forward_registry.rs`). Building a [`SshTunnelConfig`] from a stored [`TunnelConfig`]
//! and allocating the local port both live here too, since nothing else needs them.

use crate::forward_registry::{ForwardRegistry, RegistryHandle};
use crate::forward_supervisor::{BackoffPolicy, SupervisorOptions};
use crate::ssh_tunnel::{SshTunnel, SshTunnelConfig, TransientIdentityFile};
use crate::tunnel_store::{TunnelStore, TunnelStoreError};
use gpui_kit::{App, Global};
use std::io;
use std::path::Path;
use std::time::Duration;

// UNWIRED(#3): `ClusterConnection::connect` only calls `.ok()` on this today (an
// acquire failure falls back to an unbound-style direct connect rather than
// surfacing an error state), so the payload of each variant is never read. Kept
// structured, not a unit variant, for a future UI surface that reports *why* a
// tunnel failed to acquire - same shape as `TunnelStoreError`.
#[allow(dead_code)]
#[derive(Debug)]
pub enum TunnelAcquireError {
    Store(TunnelStoreError),
    Io(io::Error),
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

/// How often an established forward's `ssh` child is checked for liveness.
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
/// Reconnect backoff: starts at 1s, caps at 30s - see `BackoffPolicy::delay`.
const RECONNECT_BACKOFF: BackoffPolicy = BackoffPolicy {
    initial: Duration::from_secs(1),
    max: Duration::from_secs(30),
};

struct TunnelForwards {
    registry: ForwardRegistry<SshTunnel>,
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

/// Resolves `context`'s tunnel binding (if any) via `tunnels.toml` and acquires the
/// shared forward for it, spawning a fresh `ssh` supervisor on the first acquire for
/// that tunnel id. Returns `Ok(None)` for an unbound context - section 6.3's direct-
/// connect regression path.
pub fn acquire_for_context(
    cx: &mut App,
    tunnels_config_path: &Path,
    context: &str,
) -> Result<Option<RegistryHandle<SshTunnel>>, TunnelAcquireError> {
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
    let local_addr = crate::port_allocator::allocate()?.addr();
    let identity_file = secret
        .as_deref()
        .map(|secret| TransientIdentityFile::write(&tunnel_id, secret))
        .transpose()?;
    let mut ssh_config = SshTunnelConfig {
        bastion_user: config.bastion_user.clone(),
        bastion_host: config.bastion_host.clone(),
        bastion_port: config.bastion_port,
        jump_hosts: config.jump_hosts.clone(),
        remote_host: config.remote_host.clone(),
        remote_port: config.remote_port,
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

    let handle = registry.acquire(tunnel_id, || {
        SshTunnel::spawn(&rt, ssh_config, identity_file, local_addr, options)
    });
    Ok(Some(handle))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let path = std::env::temp_dir().join(format!(
            "fernrohr-cluster-tunnel-test-{}.toml",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        cx.update(crate::runtime::init);

        let result = cx.update(|cx| acquire_for_context(cx, &path, "no-such-context"));
        assert!(matches!(result, Ok(None)));
        // Section 6.3: an unbound context must never initialize `TunnelForwards` -
        // `ForwardRegistry` stays untouched, not just empty.
        assert!(!cx.update(|cx| cx.has_global::<TunnelForwards>()));

        let _ = std::fs::remove_file(&path);
    }
}
