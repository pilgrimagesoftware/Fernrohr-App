//! Section 2 of the tunnel-subsystem change: `ManagedForward` is the shared abstraction
//! behind both `SshTunnel` (section 3) and `K8sPortForward` (section 4) - a long-lived,
//! health-checked local TCP forward with an observable connection state and reference
//! counting so several callers can share one underlying forward.
//!
//! This module owns the trait, state machine, and the RAII acquire/release handle.
//! `ForwardRegistry` (identity-keyed sharing across multiple `ManagedForward`s) lives in
//! `forward_registry.rs`.

use std::net::SocketAddr;
use tokio::sync::watch;

/// A `ManagedForward`'s connection lifecycle. `Reconnecting` means the forward was `Up`
/// at least once and is retrying after a drop - callers should keep waiting, not tear
/// down whatever they were using the forward for.
// UNWIRED(#3): SshTunnel (section 3) and K8sPortForward (section 4) are the first
// implementations; ForwardRegistry (section 2.4) and the connect-path integration
// (section 6) are the first callers.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForwardState {
    Disconnected,
    Connecting,
    Up,
    Reconnecting,
}

/// A live reference to a `ManagedForward`, acquired via [`ManagedForward::acquire`].
/// Dropping the handle releases the reference; what "release" does (stop the forward,
/// decrement a shared refcount) is up to the callback passed to [`ForwardHandle::new`].
// UNWIRED(#3): see the note on `ForwardState`.
#[allow(dead_code)]
pub struct ForwardHandle {
    release: Option<Box<dyn FnOnce() + Send>>,
}

impl ForwardHandle {
    // UNWIRED(#3): see the note on `ForwardState`.
    #[allow(dead_code)]
    pub fn new(release: impl FnOnce() + Send + 'static) -> Self {
        Self {
            release: Some(Box::new(release)),
        }
    }
}

impl Drop for ForwardHandle {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

/// A long-lived local forward with an observable state and reference-counted lifetime.
/// Implemented by `SshTunnel` and `K8sPortForward`; `ForwardRegistry` acquires and shares
/// handles across callers keyed by tunnel/forward identity.
// UNWIRED(#3): see the note on `ForwardState`.
#[allow(dead_code)]
pub trait ManagedForward: Send + Sync {
    /// A receiver observing this forward's current state; `changed().await` waits for
    /// the next transition (e.g. `Connecting` -> `Up`).
    fn state(&self) -> watch::Receiver<ForwardState>;

    /// The local address this forward listens on. Valid once `state()` reaches `Up`
    /// (or `Reconnecting`, since the listener persists across a reconnect); undefined
    /// before the first `Up`.
    fn local_addr(&self) -> SocketAddr;

    /// Acquires a reference to this forward, starting it if this is the first acquire.
    /// Dropping the last handle releases it.
    fn acquire(&self) -> ForwardHandle;
}

/// Exercises the trait/handle contract against a fake implementation - not coverage of
/// `SshTunnel` or `K8sPortForward`, which don't exist yet (sections 3-4).
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fake `ManagedForward` whose state a test drives directly through `set_state`,
    /// and whose `acquire`/release is observable via `acquire_count`.
    struct FakeForward {
        state_tx: watch::Sender<ForwardState>,
        local_addr: SocketAddr,
        acquire_count: Arc<AtomicUsize>,
    }

    impl FakeForward {
        fn new(local_addr: SocketAddr) -> Self {
            Self {
                state_tx: watch::Sender::new(ForwardState::Disconnected),
                local_addr,
                acquire_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn set_state(&self, state: ForwardState) {
            self.state_tx.send_replace(state);
        }
    }

    impl ManagedForward for FakeForward {
        fn state(&self) -> watch::Receiver<ForwardState> {
            self.state_tx.subscribe()
        }

        fn local_addr(&self) -> SocketAddr {
            self.local_addr
        }

        fn acquire(&self) -> ForwardHandle {
            self.acquire_count.fetch_add(1, Ordering::SeqCst);
            let count = self.acquire_count.clone();
            ForwardHandle::new(move || {
                count.fetch_sub(1, Ordering::SeqCst);
            })
        }
    }

    fn local_addr() -> SocketAddr {
        "127.0.0.1:1".parse().unwrap()
    }

    #[test]
    fn state_receiver_observes_every_transition() {
        let forward = FakeForward::new(local_addr());
        let mut state = forward.state();
        assert_eq!(*state.borrow(), ForwardState::Disconnected);

        for next in [
            ForwardState::Connecting,
            ForwardState::Up,
            ForwardState::Reconnecting,
            ForwardState::Up,
        ] {
            forward.set_state(next);
            assert!(state.has_changed().unwrap());
            state.mark_unchanged();
            assert_eq!(*state.borrow(), next);
        }
    }

    #[test]
    fn local_addr_returns_the_configured_address() {
        let forward = FakeForward::new(local_addr());
        assert_eq!(forward.local_addr(), local_addr());
    }

    #[test]
    fn acquire_then_drop_releases() {
        let forward = FakeForward::new(local_addr());
        assert_eq!(forward.acquire_count.load(Ordering::SeqCst), 0);

        let handle = forward.acquire();
        assert_eq!(forward.acquire_count.load(Ordering::SeqCst), 1);

        drop(handle);
        assert_eq!(forward.acquire_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn two_acquires_track_independently() {
        let forward = FakeForward::new(local_addr());
        let first = forward.acquire();
        let second = forward.acquire();
        assert_eq!(forward.acquire_count.load(Ordering::SeqCst), 2);

        drop(first);
        assert_eq!(forward.acquire_count.load(Ordering::SeqCst), 1);

        drop(second);
        assert_eq!(forward.acquire_count.load(Ordering::SeqCst), 0);
    }
}
