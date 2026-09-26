use std::collections::HashMap;
use std::hash::Hash;

/// One key's subscription state: how many panels hold it open, and whether
/// its watch is currently suspended (section 7.1's recoverable-interruption
/// support - a bound tunnel's forward dropping to `Reconnecting`, for
/// instance, pauses every watcher on that cluster without losing the
/// refcount that would otherwise tear them down).
struct Entry {
    refcount: usize,
    // UNWIRED: section 7.2 (ConnectionHealth) is the first real caller of
    // pause/resume; only tests read this until then.
    #[allow(dead_code)]
    paused: bool,
}

/// Reference-counts watch subscriptions per key (a resource kind, for
/// `ClusterSession`). The caller starts the watch on the 0-to-1 transition
/// and tears it down on the matching 1-to-0 transition. Two windows watching
/// the same `(cluster, kind)` therefore share one underlying stream.
pub struct WatchRegistry<K> {
    entries: HashMap<K, Entry>,
}

impl<K> Default for WatchRegistry<K> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
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
        let entry = self.entries.entry(key).or_insert(Entry {
            refcount: 0,
            paused: false,
        });
        entry.refcount += 1;
        entry.refcount == 1
    }

    /// Decrements `key`'s refcount. Returns `true` on the 1-to-0 transition,
    /// when the caller should tear the watch down. Unsubscribing a key with
    /// no active subscription is a no-op. Removes the entry (dropping any
    /// paused flag) once the refcount hits zero.
    pub fn unsubscribe(&mut self, key: &K) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        entry.refcount -= 1;
        if entry.refcount == 0 {
            self.entries.remove(key);
            true
        } else {
            false
        }
    }

    pub fn refcount(&self, key: &K) -> usize {
        self.entries.get(key).map_or(0, |entry| entry.refcount)
    }

    /// Marks `key` paused. Returns `true` on the not-paused-to-paused
    /// transition, when the caller should stop consuming the underlying
    /// stream (without unsubscribing - the refcount is untouched). A no-op,
    /// returning `false`, on an already-paused or untracked key.
    pub fn pause(&mut self, key: &K) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        if entry.paused {
            false
        } else {
            entry.paused = true;
            true
        }
    }

    /// Marks `key` resumed. Returns `true` on the paused-to-not-paused
    /// transition, when the caller should relist and reconverge the
    /// underlying stream. A no-op, returning `false`, on an already-active
    /// or untracked key.
    pub fn resume(&mut self, key: &K) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        if entry.paused {
            entry.paused = false;
            true
        } else {
            false
        }
    }

    pub fn is_paused(&self, key: &K) -> bool {
        self.entries.get(key).is_some_and(|entry| entry.paused)
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

    /// A fake watcher driven purely by `WatchRegistry`'s pause/resume return
    /// values - proving the composition a real caller (section 7.2's
    /// `ConnectionHealth`) will use, the same way section 2.3's fake
    /// transport proved `ForwardSupervisor`'s composition before any real
    /// transport existed.
    #[derive(Default)]
    struct FakeWatcher {
        consuming: bool,
        relists: usize,
    }

    #[test]
    fn paused_watcher_stops_consuming_and_resume_relists_and_reconverges() {
        let mut registry = WatchRegistry::new();
        let mut watcher = FakeWatcher::default();

        registry.subscribe("pods");
        watcher.consuming = true;

        assert!(registry.pause(&"pods"), "first pause should transition");
        if registry.is_paused(&"pods") {
            watcher.consuming = false;
        }
        assert!(!watcher.consuming, "paused watcher must stop consuming");

        assert!(
            !registry.pause(&"pods"),
            "pausing an already-paused key is a no-op"
        );

        assert!(registry.resume(&"pods"), "resume should transition back");
        if !registry.is_paused(&"pods") {
            watcher.relists += 1;
            watcher.consuming = true;
        }
        assert_eq!(watcher.relists, 1, "resume must relist exactly once");
        assert!(watcher.consuming, "resumed watcher must reconverge");

        assert!(
            !registry.resume(&"pods"),
            "resuming an already-active key is a no-op"
        );
    }

    #[test]
    fn pause_and_resume_of_untracked_key_are_no_ops() {
        let mut registry: WatchRegistry<&str> = WatchRegistry::new();
        assert!(!registry.pause(&"pods"));
        assert!(!registry.resume(&"pods"));
        assert!(!registry.is_paused(&"pods"));
    }

    #[test]
    fn teardown_while_paused_drops_the_paused_flag() {
        let mut registry = WatchRegistry::new();
        registry.subscribe("pods");
        registry.pause(&"pods");

        assert!(registry.unsubscribe(&"pods"), "last unsubscribe tears down");
        assert!(!registry.is_paused(&"pods"));

        registry.subscribe("pods");
        assert!(
            !registry.is_paused(&"pods"),
            "a fresh subscription after teardown must not inherit the old pause"
        );
    }
}
