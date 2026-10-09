//! A context's retained events, kept current by the shared Event watch, with
//! the relist sweep every watched table needs.

use super::row::EventRow;
use crate::k8s::resource::load_phase::LoadPhase;
use crate::util::resource_index::ResourceIndex;
use k8s_openapi::api::core::v1::Event as K8sEvent;
use kube_runtime::watcher;
use std::collections::HashSet;

/// One context's events, kept up to date by [`EventsTable::apply`] as watch
/// events arrive. The same relist handling as the other watched tables: an
/// event that expired while the watch was down never gets a `Delete`, so
/// `InitDone` sweeps anything the relist didn't see.
#[derive(Default)]
pub struct EventsTable {
    index: ResourceIndex<EventRow>,
    /// Uids seen so far in the current `Init..InitDone` relist, if one is running.
    relisting: Option<HashSet<String>>,
    /// The API server's refusal, when it won't let this user list events.
    refused: Option<String>,
    /// Where the table is in its lists (`list-loading-indicator`).
    phase: LoadPhase,
}

impl EventsTable {
    pub fn rows(&self) -> &[EventRow] {
        self.index.items()
    }

    /// Why events can't be listed, if the server refused.
    pub fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// Where the table is in its lists.
    // UNWIRED(#208): the list panels' loading and refreshing states read it.
    #[allow(dead_code)]
    pub fn phase(&self) -> LoadPhase {
        self.phase
    }

    pub fn set_refused(&mut self, message: String) {
        self.refused = Some(message);
    }

    /// Applies one watch event: `Apply`/`InitApply` upsert by uid, `Delete`
    /// removes by uid, and `InitDone` ends a relist by removing what it didn't
    /// see.
    pub fn apply(&mut self, event: watcher::Event<K8sEvent>) {
        match event {
            watcher::Event::Apply(event) => {
                let row = EventRow::new(&event);
                self.index.apply_applied(row.uid.clone(), row);
            }
            watcher::Event::InitApply(event) => {
                let row = EventRow::new(&event);
                if let Some(seen) = &mut self.relisting {
                    seen.insert(row.uid.clone());
                }
                self.phase.received();
                self.index.apply_applied(row.uid.clone(), row);
            }
            watcher::Event::Delete(event) => {
                self.index
                    .apply_deleted(event.metadata.uid.as_deref().unwrap_or_default());
            }
            watcher::Event::Init => {
                self.refused = None;
                self.relisting = Some(HashSet::new());
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
                    .map(|row| row.uid.clone())
                    .filter(|uid| !seen.contains(uid))
                    .collect();
                for uid in stale {
                    self.index.apply_deleted(&uid);
                }
                self.phase.done();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EventsTable;
    use crate::k8s::resource::load_phase::LoadPhase;
    use kube_runtime::watcher;

    /// `list-loading-indicator` 1.1: the events table's phase follows its
    /// lists, as the object list's and the Pods table's do.
    #[test]
    fn the_phase_follows_a_first_list_and_a_relist() {
        let mut table = EventsTable::default();
        table.apply(watcher::Event::Init);
        assert_eq!(table.phase(), LoadPhase::FirstLoad { received: 0 });
        table.apply(watcher::Event::InitDone);
        assert_eq!(table.phase(), LoadPhase::Loaded);
        table.apply(watcher::Event::Init);
        assert_eq!(table.phase(), LoadPhase::Refreshing { received: 0 });
    }
}
