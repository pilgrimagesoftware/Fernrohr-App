//! Section 2.4 of the tunnel-subsystem change: shares one `ManagedForward` instance
//! across every caller that acquires the same key (a [`crate::k8s::cluster::tunnel::
//! ForwardKey`] for an SSH tunnel, or a Pod/Service port-forward key) - two Pods panels
//! watching the same tunnel-bound context get one underlying `ssh`, not two.
//!
//! Sharing is `Weak`-based: the registry holds only a [`Weak`] per key, so the shared
//! `F` is torn down (via its own `Drop`) the instant the last [`RegistryHandle`] for
//! that key is dropped, rather than lingering as a stopped-but-cached object.
//!
//! Section 2.2 of `tunnel-management-ui` generalized the key from a bare tunnel-id
//! `String` to `K: Hash + Eq + Clone + Ord` (SSH forwards key by tunnel *and* target, so
//! one bastion reaching several clusters gets one `ssh` per cluster, not one for the
//! whole bastion) and added a `watch`-published live key set, so a Tunnels UI can derive
//! "running" state without polling.

use crate::forward::managed::{ForwardHandle, ManagedForward};
use parking_lot::Mutex;
use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;
use std::sync::{Arc, Weak};
use tokio::sync::watch;

/// Shared state behind [`ForwardRegistry`], held in an `Arc` so a [`RegistryHandle`] can
/// clean up its entry and update the live key set on `Drop` without borrowing the
/// registry that created it.
struct Inner<K, F: ManagedForward> {
    entries: Mutex<HashMap<K, Weak<F>>>,
    live_keys: watch::Sender<BTreeSet<K>>,
}

pub struct ForwardRegistry<K, F: ManagedForward> {
    inner: Arc<Inner<K, F>>,
}

/// A reference to a shared forward. Dropping it releases both the forward's own
/// acquire/release contract and, once the last one drops, the registry's share of it
/// (removing the entry and its key from the live set).
pub struct RegistryHandle<K: Hash + Eq + Clone + Ord, F: ManagedForward> {
    forward: Arc<F>,
    // Held only for its Drop side effect (delegates to the forward's own
    // acquire/release bookkeeping); never read directly.
    _forward_handle: ForwardHandle,
    key: K,
    inner: Arc<Inner<K, F>>,
}

impl<K: Hash + Eq + Clone + Ord, F: ManagedForward> RegistryHandle<K, F> {
    pub fn forward(&self) -> &F {
        &self.forward
    }
}

impl<K: Hash + Eq + Clone + Ord, F: ManagedForward> Drop for RegistryHandle<K, F> {
    /// Locks the same `entries` map [`ForwardRegistry::acquire`] locks, so a concurrent
    /// acquire for this key either observes the `Weak` before it's removed (and
    /// upgrades it, since our `Arc` is still alive while we hold the lock) or observes
    /// no entry at all (and creates a fresh one) - never a removed-but-still-upgradable
    /// half-state.
    fn drop(&mut self) {
        let mut entries = self.inner.entries.lock();
        if Arc::strong_count(&self.forward) == 1 {
            entries.remove(&self.key);
            let key = self.key.clone();
            drop(entries);
            self.inner.live_keys.send_modify(|keys| {
                keys.remove(&key);
            });
        }
    }
}

impl<K: Hash + Eq + Clone + Ord, F: ManagedForward> Default for ForwardRegistry<K, F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone + Ord, F: ManagedForward> ForwardRegistry<K, F> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                entries: Mutex::new(HashMap::new()),
                live_keys: watch::Sender::new(BTreeSet::new()),
            }),
        }
    }

    /// Returns the shared forward for `key`, calling `factory` to create it if no live
    /// instance exists yet (either never created, or its last handle was already
    /// dropped).
    pub fn acquire(&self, key: K, factory: impl FnOnce() -> F) -> RegistryHandle<K, F> {
        let mut entries = self.inner.entries.lock();

        let forward = match entries.get(&key).and_then(Weak::upgrade) {
            Some(forward) => forward,
            None => {
                let forward = Arc::new(factory());
                entries.insert(key.clone(), Arc::downgrade(&forward));
                self.inner.live_keys.send_modify(|keys| {
                    keys.insert(key.clone());
                });
                forward
            }
        };
        drop(entries);

        let forward_handle = forward.acquire();
        RegistryHandle {
            forward,
            _forward_handle: forward_handle,
            key,
            inner: self.inner.clone(),
        }
    }

    /// The live set of acquired keys, updated on first acquire and last release - a
    /// Tunnels UI derives each tunnel's running state from this without polling, via
    /// `k8s::cluster::tunnel::live_forward_keys`.
    pub fn live_keys(&self) -> watch::Receiver<BTreeSet<K>> {
        self.inner.live_keys.subscribe()
    }
}

/// Exercises key-based sharing and teardown against a fake `ManagedForward` - not
/// coverage of `SshTunnel` or `K8sPortForward` (sections 3-4), which don't exist yet.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::forward::managed::ForwardState;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    /// A key shaped like the SSH path's real `ForwardKey`: sharing a `tunnel_id` alone
    /// must not be enough to share a forward when the target differs.
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    struct TestKey {
        tunnel_id: &'static str,
        target: &'static str,
    }

    #[test]
    fn two_acquirers_of_the_same_key_share_one_forward() {
        let registry: ForwardRegistry<String, CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let first = registry.acquire(
            "ctx-a".to_string(),
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        let second = registry.acquire(
            "ctx-a".to_string(),
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
    fn different_keys_get_independent_forwards() {
        let registry: ForwardRegistry<String, CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let a = registry.acquire(
            "ctx-a".to_string(),
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        let b = registry.acquire(
            "ctx-b".to_string(),
            counting_factory(constructed.clone(), torn_down.clone()),
        );

        assert_eq!(constructed.load(Ordering::SeqCst), 2);
        assert!(!Arc::ptr_eq(&a.forward, &b.forward));
    }

    /// Tasks.md 2.2: two keys sharing a `tunnel_id` but pointing at different targets
    /// must make two entries, not share one - the whole point of keying by target too.
    #[test]
    fn two_keys_with_the_same_tunnel_id_but_different_targets_make_two_entries() {
        let registry: ForwardRegistry<TestKey, CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let a = registry.acquire(
            TestKey {
                tunnel_id: "qa-bastion",
                target: "10.0.0.1:443",
            },
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        let b = registry.acquire(
            TestKey {
                tunnel_id: "qa-bastion",
                target: "10.0.0.2:443",
            },
            counting_factory(constructed.clone(), torn_down.clone()),
        );

        assert_eq!(constructed.load(Ordering::SeqCst), 2);
        assert!(!Arc::ptr_eq(&a.forward, &b.forward));
    }

    #[test]
    fn forward_tears_down_only_after_the_last_handle_drops() {
        let registry: ForwardRegistry<String, CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let first = registry.acquire(
            "ctx-a".to_string(),
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        let second = registry.acquire(
            "ctx-a".to_string(),
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
        let registry: ForwardRegistry<String, CountingForward> = ForwardRegistry::new();
        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let first = registry.acquire(
            "ctx-a".to_string(),
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        drop(first);
        assert_eq!(torn_down.load(Ordering::SeqCst), 1);

        let second = registry.acquire(
            "ctx-a".to_string(),
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

    /// Tasks.md 2.2: the watched live-key set gains a key on its first acquire and
    /// loses it only once the last handle for it drops, even while a second handle for
    /// the same key is still live.
    #[test]
    fn live_keys_reflects_first_acquire_and_last_release() {
        let registry: ForwardRegistry<String, CountingForward> = ForwardRegistry::new();
        let live = registry.live_keys();
        assert!(live.borrow().is_empty());

        let constructed = Arc::new(AtomicUsize::new(0));
        let torn_down = Arc::new(AtomicUsize::new(0));

        let first = registry.acquire(
            "ctx-a".to_string(),
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        assert!(live.borrow().contains("ctx-a"));

        let second = registry.acquire(
            "ctx-a".to_string(),
            counting_factory(constructed.clone(), torn_down.clone()),
        );
        assert_eq!(live.borrow().len(), 1, "still just one live key");

        drop(first);
        assert!(
            live.borrow().contains("ctx-a"),
            "one live handle still remains"
        );

        drop(second);
        assert!(live.borrow().is_empty(), "last handle dropped the key too");
    }
}
