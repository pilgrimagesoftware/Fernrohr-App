//! The live set of pods one watch keeps up to date, including the relist sweep that removes pods deleted while the watch was down.

use super::*;

/// The live Pods index for one watch, kept up to date by [`PodsTable::apply`]
/// as `watcher::Event`s arrive off the drain.
#[derive(Default)]
pub struct PodsTable {
    pub(super) index: ResourceIndex<Pod>,
    /// Uids seen so far in the current `Init..InitDone` relist, if one is in
    /// progress. `kube_runtime::watcher` handles the actual reconnect/relist
    /// (design D3) and restarts this cycle after a terminal error; a pod
    /// deleted while disconnected never gets an explicit `Delete` - it's
    /// simply absent from the relist - so `InitDone` sweeps anything not
    /// seen during the cycle.
    pub(super) relisting: Option<std::collections::HashSet<String>>,
}

impl PodsTable {
    // UNWIRED: `PodsPanel::new` builds this via `PodsTable::default`; only
    // this module's own tests call `new`.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pods(&self) -> &[Pod] {
        self.index.items()
    }

    /// Applies one watch event. `Apply`/`InitApply` upsert by uid, `Delete`
    /// removes by uid. `Init` starts a relist cycle; `InitDone` ends it and
    /// removes anything present before the relist that wasn't re-seen during
    /// it, converging the table to the post-reconnect cluster state.
    pub fn apply(&mut self, event: watcher::Event<Pod>) {
        match event {
            watcher::Event::Apply(pod) => {
                self.index.apply_applied(uid(&pod), pod);
            }
            watcher::Event::InitApply(pod) => {
                let id = uid(&pod);
                if let Some(seen) = &mut self.relisting {
                    seen.insert(id.clone());
                }
                self.index.apply_applied(id, pod);
            }
            watcher::Event::Delete(pod) => {
                self.index.apply_deleted(&uid(&pod));
            }
            watcher::Event::Init => {
                self.relisting = Some(std::collections::HashSet::new());
            }
            watcher::Event::InitDone => {
                let Some(seen) = self.relisting.take() else {
                    return;
                };
                let stale: Vec<String> = self
                    .index
                    .items()
                    .iter()
                    .map(uid)
                    .filter(|id| !seen.contains(id))
                    .collect();
                for id in stale {
                    self.index.apply_deleted(&id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
