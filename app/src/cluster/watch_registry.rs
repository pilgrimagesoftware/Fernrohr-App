use std::collections::HashMap;
use std::hash::Hash;

/// Reference-counts watch subscriptions per key (a resource kind, for
/// `ClusterSession`). The caller starts the watch on the 0-to-1 transition
/// and tears it down on the matching 1-to-0 transition. Two windows watching
/// the same `(cluster, kind)` therefore share one underlying stream.
pub struct WatchRegistry<K> {
    refcounts: HashMap<K, usize>,
}

impl<K> Default for WatchRegistry<K> {
    fn default() -> Self {
        Self {
            refcounts: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash> WatchRegistry<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments `key`'s refcount. Returns `true` on the 0-to-1 transition,
    /// when the caller should start the watch.
    pub fn subscribe(&mut self, key: K) -> bool {
        let count = self.refcounts.entry(key).or_insert(0);
        *count += 1;
        *count == 1
    }

    /// Decrements `key`'s refcount. Returns `true` on the 1-to-0 transition,
    /// when the caller should tear the watch down. Unsubscribing a key with
    /// no active subscription is a no-op.
    pub fn unsubscribe(&mut self, key: &K) -> bool {
        let Some(count) = self.refcounts.get_mut(key) else {
            return false;
        };
        *count -= 1;
        if *count == 0 {
            self.refcounts.remove(key);
            true
        } else {
            false
        }
    }

    // UNWIRED on the non-test bin target: only tests read the refcount
    // directly today; a future status row is the first production caller.
    #[allow(dead_code)]
    pub fn refcount(&self, key: &K) -> usize {
        self.refcounts.get(key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_subscribers_share_one_stream() {
        let mut registry = WatchRegistry::new();

        assert!(registry.subscribe("pods"), "first subscribe should start");
        assert!(
            !registry.subscribe("pods"),
            "second subscribe should not start a new stream"
        );
        assert_eq!(registry.refcount(&"pods"), 2);
    }

    #[test]
    fn teardown_runs_only_on_last_unsubscribe() {
        let mut registry = WatchRegistry::new();
        registry.subscribe("pods");
        registry.subscribe("pods");

        assert!(
            !registry.unsubscribe(&"pods"),
            "first unsubscribe of two must not tear down"
        );
        assert_eq!(registry.refcount(&"pods"), 1);

        assert!(
            registry.unsubscribe(&"pods"),
            "last unsubscribe must tear down"
        );
        assert_eq!(registry.refcount(&"pods"), 0);
    }

    #[test]
    fn unsubscribe_of_untracked_key_is_a_no_op() {
        let mut registry: WatchRegistry<&str> = WatchRegistry::new();
        assert!(!registry.unsubscribe(&"pods"));
        assert_eq!(registry.refcount(&"pods"), 0);
    }
}
