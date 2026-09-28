//! Section 2.4 of the tunnel-subsystem change: shares one `ManagedForward` instance
//! across every caller that acquires the same identity (a tunnel id, or a
//! Pod/Service port-forward key) - two Pods panels watching the same tunnel-bound
//! context get one underlying `ssh`, not two.
//!
//! Sharing is `Weak`-based: the registry holds only a [`Weak`] per identity, so the
//! shared `F` is torn down (via its own `Drop`) the instant the last [`RegistryHandle`]
//! for that identity is dropped, rather than lingering as a stopped-but-cached object.

use crate::managed_forward::{ForwardHandle, ManagedForward};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

// UNWIRED(#3): the connect-path integration (section 6.2) is the first real caller,
// acquiring a forward keyed by tunnel id.
#[allow(dead_code)]
pub struct ForwardRegistry<F: ManagedForward> {
    entries: Mutex<HashMap<String, Weak<F>>>,
}

/// A reference to a shared forward. Dropping it releases both the forward's own
/// acquire/release contract and, once the last one drops, the registry's share of it.
#[allow(dead_code)]
pub struct RegistryHandle<F: ManagedForward> {
    forward: Arc<F>,
    // Held only for its Drop side effect (delegates to the forward's own
    // acquire/release bookkeeping); never read directly.
    _forward_handle: ForwardHandle,
}

impl<F: ManagedForward> RegistryHandle<F> {
    #[allow(dead_code)]
    pub fn forward(&self) -> &F {
        &self.forward
    }
}

impl<F: ManagedForward> Default for ForwardRegistry<F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: ManagedForward> ForwardRegistry<F> {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the shared forward for `identity`, calling `factory` to create it if no
    /// live instance exists yet (either never created, or its last handle was already
    /// dropped).
    #[allow(dead_code)]
    pub fn acquire(
        &self,
        identity: impl Into<String>,
        factory: impl FnOnce() -> F,
    ) -> RegistryHandle<F> {
        let identity = identity.into();
        let mut entries = self.entries.lock();

        let forward = match entries.get(&identity).and_then(Weak::upgrade) {
            Some(forward) => forward,
            None => {
                let forward = Arc::new(factory());
                entries.insert(identity, Arc::downgrade(&forward));
                forward
            }
        };
        drop(entries);

        let forward_handle = forward.acquire();
        RegistryHandle {
            forward,
            _forward_handle: forward_handle,
        }
    }
}

/// Exercises identity-keyed sharing and teardown against a fake `ManagedForward` -
/// not coverage of `SshTunnel` or `K8sPortForward` (sections 3-4), which don't exist yet.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_forward::ForwardState;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::watch;

    /// Counts constructions (via the shared `constructed` counter) and marks
    /// `torn_down` on `Drop`, so a test can observe both registry-level sharing and
    /// the underlying instance's teardown.
    struct CountingForward {
        state_tx: watch::Sender<ForwardState>,
        torn_down: Arc<AtomicUsize>,
    }

    impl Drop for CountingForward {
        fn drop(&mut self) {
            self.torn_down.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl ManagedForward for CountingForward {
        fn state(&self) -> watch::Receiver<ForwardState> {
            self.state_tx.subscribe()
        }

        fn local_addr(&self) -> SocketAddr {
            "127.0.0.1:1".parse().unwrap()
        }

        fn acquire(&self) -> ForwardHandle {
            ForwardHandle::new(|| {})
        }
    }

    fn counting_factory(
        constructed: Arc<AtomicUsize>,
        torn_down: Arc<AtomicUsize>,
    ) -> impl FnOnce() -> CountingForward {
        move || {
            constructed.fetch_add(1, Ordering::SeqCst);
            CountingForward {
                state_tx: watch::Sender::new(ForwardState::Disconnected),
                torn_down,
            }
        }
    }

    #[test]
    fn two_acquirers_of_the_same_identity_share_one_forward() {
        let registry: ForwardRegistry<CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let first = registry.acquire(
            "ctx-a",
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        let second = registry.acquire(
            "ctx-a",
            counting_factory(constructed.clone(), torn_down.clone()),
        );

        assert_eq!(
            constructed.load(Ordering::SeqCst),
            1,
            "factory should run once"
        );
        assert!(Arc::ptr_eq(&first.forward, &second.forward));
    }

    #[test]
    fn different_identities_get_independent_forwards() {
        let registry: ForwardRegistry<CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let a = registry.acquire(
            "ctx-a",
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        let b = registry.acquire(
            "ctx-b",
            counting_factory(constructed.clone(), torn_down.clone()),
        );

        assert_eq!(constructed.load(Ordering::SeqCst), 2);
        assert!(!Arc::ptr_eq(&a.forward, &b.forward));
    }

    #[test]
    fn forward_tears_down_only_after_the_last_handle_drops() {
        let registry: ForwardRegistry<CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let first = registry.acquire(
            "ctx-a",
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        let second = registry.acquire(
            "ctx-a",
            counting_factory(constructed.clone(), torn_down.clone()),
        );

        drop(first);
        assert_eq!(
            torn_down.load(Ordering::SeqCst),
            0,
            "one live handle remains"
        );

        drop(second);
        assert_eq!(torn_down.load(Ordering::SeqCst), 1, "last handle dropped");
    }

    #[test]
    fn a_new_acquire_after_teardown_creates_a_fresh_forward() {
        let registry: ForwardRegistry<CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let first = registry.acquire(
            "ctx-a",
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        drop(first);
        assert_eq!(torn_down.load(Ordering::SeqCst), 1);

        let second = registry.acquire(
            "ctx-a",
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        assert_eq!(
            constructed.load(Ordering::SeqCst),
            2,
            "stale entry must not be reused"
        );
        drop(second);
        assert_eq!(torn_down.load(Ordering::SeqCst), 2);
    }
}
