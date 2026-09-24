use super::watch_registry::WatchRegistry;
use kube::Client;

/// One connected cluster: its client plus the reference-counted set of kind
/// watches currently open across every window. App-scoped (not per-window),
/// so two panels showing Pods for the same cluster share one watch.
pub struct ClusterSession {
    pub client: Client,
    watches: WatchRegistry<String>,
}

impl ClusterSession {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            watches: WatchRegistry::new(),
        }
    }

    /// Subscribes a panel to `kind`. `start` runs only when this is the
    /// first subscriber for `kind` and must return the teardown to run when
    /// the last subscriber unsubscribes.
    pub fn subscribe(
        &mut self,
        kind: impl Into<String>,
        start: impl FnOnce() -> Box<dyn FnOnce()>,
    ) {
        self.watches.subscribe(kind.into(), start);
    }

    pub fn unsubscribe(&mut self, kind: &str) {
        self.watches.unsubscribe(&kind.to_string());
    }

    pub fn watch_refcount(&self, kind: &str) -> usize {
        self.watches.refcount(&kind.to_string())
    }
}
