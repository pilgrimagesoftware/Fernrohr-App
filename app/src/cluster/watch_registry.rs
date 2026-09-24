use std::collections::HashMap;
use std::hash::Hash;

struct Subscription {
    refcount: usize,
    stop: Box<dyn FnOnce()>,
}

/// Reference-counts watch subscriptions per key (a resource kind, for
/// `ClusterSession`). The first `subscribe` for a key starts the watch; later
/// subscribers just bump the count. The last matching `unsubscribe` tears the
/// watch down. Two windows watching the same `(cluster, kind)` therefore
/// share one underlying stream.
pub struct WatchRegistry<K> {
    subscriptions: HashMap<K, Subscription>,
}

impl<K> Default for WatchRegistry<K> {
    fn default() -> Self {
        Self {
            subscriptions: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash> WatchRegistry<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to `key`. `start` runs only on the 0-to-1 transition and
    /// must return the teardown to run on the matching 1-to-0 transition.
    pub fn subscribe(&mut self, key: K, start: impl FnOnce() -> Box<dyn FnOnce()>) {
        match self.subscriptions.get_mut(&key) {
            Some(subscription) => subscription.refcount += 1,
            None => {
                let stop = start();
                self.subscriptions
                    .insert(key, Subscription { refcount: 1, stop });
            }
        }
    }

    /// Decrements `key`'s refcount; on reaching zero, removes the
    /// subscription and runs its teardown. Unsubscribing a key with no
    /// active subscription is a no-op.
    pub fn unsubscribe(&mut self, key: &K) {
        let Some(subscription) = self.subscriptions.get_mut(key) else {
            return;
        };
        subscription.refcount -= 1;
        if subscription.refcount == 0
            && let Some(subscription) = self.subscriptions.remove(key)
        {
            (subscription.stop)();
        }
    }

    pub fn refcount(&self, key: &K) -> usize {
        self.subscriptions.get(key).map_or(0, |s| s.refcount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn counting_start(starts: Rc<Cell<u32>>, stops: Rc<Cell<u32>>) -> Box<dyn FnOnce()> {
        starts.set(starts.get() + 1);
        Box::new(move || stops.set(stops.get() + 1))
    }

    #[test]
    fn two_subscribers_share_one_stream() {
        let starts = Rc::new(Cell::new(0));
        let stops = Rc::new(Cell::new(0));
        let mut registry = WatchRegistry::new();

        registry.subscribe("pods", {
            let (starts, stops) = (starts.clone(), stops.clone());
            move || counting_start(starts, stops)
        });
        registry.subscribe("pods", {
            let (starts, stops) = (starts.clone(), stops.clone());
            move || counting_start(starts, stops)
        });

        assert_eq!(
            starts.get(),
            1,
            "second subscribe should not start a new stream"
        );
        assert_eq!(registry.refcount(&"pods"), 2);
        assert_eq!(stops.get(), 0);
    }

    #[test]
    fn teardown_runs_only_on_last_unsubscribe() {
        let starts = Rc::new(Cell::new(0));
        let stops = Rc::new(Cell::new(0));
        let mut registry = WatchRegistry::new();

        registry.subscribe("pods", {
            let (starts, stops) = (starts.clone(), stops.clone());
            move || counting_start(starts, stops)
        });
        registry.subscribe("pods", {
            let (starts, stops) = (starts.clone(), stops.clone());
            move || counting_start(starts, stops)
        });

        registry.unsubscribe(&"pods");
        assert_eq!(
            stops.get(),
            0,
            "first unsubscribe of two must not tear down"
        );
        assert_eq!(registry.refcount(&"pods"), 1);

        registry.unsubscribe(&"pods");
        assert_eq!(stops.get(), 1, "last unsubscribe must tear down");
        assert_eq!(registry.refcount(&"pods"), 0);
    }

    #[test]
    fn unsubscribe_of_untracked_key_is_a_no_op() {
        let mut registry: WatchRegistry<&str> = WatchRegistry::new();
        registry.unsubscribe(&"pods");
        assert_eq!(registry.refcount(&"pods"), 0);
    }
}
