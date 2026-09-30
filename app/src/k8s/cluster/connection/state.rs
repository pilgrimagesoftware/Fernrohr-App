//! The `ConnectionState` enum and the `ClusterConnection` GPUI entity itself: its
//! fields, test constructors, and read-only accessors (`since`, `forward_state`,
//! `forward_wait`). The connect path (`ClusterConnection::connect`) and its
//! supporting functions are `connect.rs`'s; probing is `probe.rs`'s.

use super::*;

#[derive(Clone)]
pub enum ConnectionState {
    Connecting,
    /// The context is bound to a tunnel whose forward hasn't reached `Up` yet. Distinct
    /// from `Connecting` so a view can show "waiting for tunnel" rather than a generic
    /// spinner - the wait here is for `ssh`, not for the API server itself.
    WaitingForTunnel,
    Connected(Client),
    Failed(String),
}

/// A GPUI entity exposing connection state for one cluster context, so a
/// view can observe it and re-render as the connection progresses.
pub struct ClusterConnection {
    pub state: ConnectionState,
    /// When `state` last changed - `connection-status-bar` design.md decision 1's source
    /// for the status bar's "waiting for tunnel" and "failed" elapsed times (`Paused`'s own
    /// elapsed comes from `WatchRegistry::first_paused` instead, since that state lives
    /// beside the watch it paused rather than here).
    pub(super) since: Instant,
    /// Kept alive for as long as this connection exists - section 6.2's "release the
    /// forward when the last session using it disconnects" is just this field's own
    /// `Drop` (via `RegistryHandle`/`SshTunnel`'s), since there is one `ClusterSession`
    /// (and so one `ClusterConnection`) per app today. `None` for an unbound context.
    pub(super) _forward: Option<RegistryHandle<ForwardKey, SshTunnel>>,
}

impl ClusterConnection {
    /// A connection in a chosen state with no forward, for tests elsewhere in the crate
    /// (e.g. `picker.rs`'s "picker stays interactive after a failed connection" case)
    /// that need a `ClusterConnection` in `Failed`/`Connected`/etc. without driving a
    /// real [`connect`](Self::connect) - which would need a runtime, a kubeconfig, and
    /// a reachable (or deliberately unreachable) server.
    #[cfg(test)]
    pub(crate) fn test_with_state(state: ConnectionState) -> Self {
        Self {
            state,
            since: Instant::now(),
            _forward: None,
        }
    }

    /// [`Self::test_with_state`] bound to a real tunnel forward - the seam
    /// `ClusterRegistry`'s hold/release tests use to observe a tunnel actually torn
    /// down (its `ForwardRegistry` entry gone) when the last window holding its
    /// context releases it, without driving a real [`Self::connect`].
    #[cfg(test)]
    pub(crate) fn test_with_state_and_forward(
        state: ConnectionState,
        forward: RegistryHandle<ForwardKey, SshTunnel>,
    ) -> Self {
        Self {
            state,
            since: Instant::now(),
            _forward: Some(forward),
        }
    }

    /// When [`Self::state`] last changed, for the status bar's elapsed-time display.
    pub fn since(&self) -> Instant {
        self.since
    }

    /// The bound forward's state receiver, for section 7.2's `ConnectionHealth` to watch -
    /// `None` for an unbound context, which has no forward to go unhealthy.
    pub fn forward_state(&self) -> Option<watch::Receiver<ForwardState>> {
        self._forward
            .as_ref()
            .map(|handle| handle.forward().state())
    }

    /// The state receiver and local address together, for section 7.3's credential-refresh
    /// path - it needs both to call [`connect_and_probe`] again without re-resolving the
    /// context's tunnel binding, exactly what [`ClusterConnection::connect`] captured at the
    /// original connect.
    pub(in crate::k8s::cluster) fn forward_wait(
        &self,
    ) -> Option<(watch::Receiver<ForwardState>, SocketAddr)> {
        self._forward
            .as_ref()
            .map(|handle| (handle.forward().state(), handle.forward().local_addr()))
    }
}
