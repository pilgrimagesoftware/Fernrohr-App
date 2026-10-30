//! The live set of pods one watch keeps up to date, including the relist sweep that removes pods deleted while the watch was down.

use super::*;
use crate::k8s::resource::load_phase::LoadPhase;
use crate::ui::row_flash::RowChange;

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
    /// Where the table is in its lists (`list-loading-indicator`). Once a
    /// list has completed, an absent pod means gone rather than not listed
    /// yet - what a detail panel reading its pod from here needs to tell apart
    /// (`live-detail-panels` D2).
    phase: LoadPhase,
    /// The live changes the Pods tables flash (#275).
    journal: crate::ui::row_flash::ChangeJournal,
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

    /// Whether the table has finished its first list.
    pub fn synced(&self) -> bool {
        self.phase.has_loaded()
    }

    /// Where the table is in its lists.
    pub fn phase(&self) -> LoadPhase {
        self.phase
    }

    /// The live changes a table flashes (#275).
    pub(crate) fn journal(&self) -> &crate::ui::row_flash::ChangeJournal {
        &self.journal
    }

    /// Whether a watch event now is a live change - after the first list and
    /// outside a relist - rather than part of a list, which never flashes.
    fn live(&self) -> bool {
        self.relisting.is_none() && self.phase.has_loaded()
    }

    /// The pod named `name` in `namespace`, preferring the one with `uid` when
    /// a delete and a recreate under the same name briefly overlap.
    pub fn find(&self, namespace: &str, name: &str, uid: Option<&str>) -> Option<&Pod> {
        let mut named = self.pods().iter().filter(|pod| {
            pod.metadata.namespace.as_deref() == Some(namespace)
                && pod.metadata.name.as_deref() == Some(name)
        });
        let first = named.next()?;
        if uid.is_none() || first.metadata.uid.as_deref() == uid {
            return Some(first);
        }
        Some(
            named
                .find(|pod| pod.metadata.uid.as_deref() == uid)
                .unwrap_or(first),
        )
    }

    /// Applies one watch event. `Apply`/`InitApply` upsert by uid, `Delete`
    /// removes by uid. `Init` starts a relist cycle; `InitDone` ends it and
    /// removes anything present before the relist that wasn't re-seen during
    /// it, converging the table to the post-reconnect cluster state.
    pub fn apply(&mut self, event: watcher::Event<Pod>) {
        match event {
            watcher::Event::Apply(pod) => {
                let id = uid(&pod);
                if self.live() {
                    let change = if self.index.contains(&id) {
                        RowChange::Modified
                    } else {
                        RowChange::Added
                    };
                    self.journal.record(id.clone(), change);
                }
                self.index.apply_applied(id, pod);
            }
            watcher::Event::InitApply(pod) => {
                let id = uid(&pod);
                if let Some(seen) = &mut self.relisting {
                    seen.insert(id.clone());
                }
                self.phase.received();
                self.index.apply_applied(id, pod);
            }
            watcher::Event::Delete(pod) => {
                let id = uid(&pod);
                if self.live() && self.index.contains(&id) {
                    self.journal.record(id.clone(), RowChange::Deleted);
                }
                self.index.apply_deleted(&id);
            }
            watcher::Event::Init => {
                self.relisting = Some(std::collections::HashSet::new());
                self.phase.init();
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
                self.phase.done();
            }
        }
    }
}

#[cfg(test)]
mod tests;
