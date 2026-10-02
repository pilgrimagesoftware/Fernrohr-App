//! The live set of one kind's objects that one watch keeps up to date, including the
//! relist sweep, and the refusal a forbidden kind shows in place of rows.

use super::row::ObjectRow;
use crate::util::resource_index::ResourceIndex;
use kube::api::DynamicObject;
use kube_runtime::watcher;
use std::collections::HashSet;

/// One kind's live rows for one watch, kept up to date by [`ObjectsTable::apply`] as
/// `watcher::Event`s arrive off the drain. The same relist handling as the Pods
/// table: an object deleted while the watch was down never gets an explicit `Delete`,
/// so `InitDone` sweeps anything the relist didn't see.
#[derive(Default)]
pub struct ObjectsTable {
    index: ResourceIndex<ObjectRow>,
    /// Uids seen so far in the current `Init..InitDone` relist, if one is running.
    relisting: Option<HashSet<String>>,
    /// The API server's refusal, when it won't let this user list the kind (a 403).
    refused: Option<String>,
}

impl ObjectsTable {
    // UNWIRED: `ObjectListPanel` (`standard-resource-panels` 1.3) is the first
    // caller; until then only tests use it.
    #[allow(dead_code)]
    pub fn rows(&self) -> &[ObjectRow] {
        self.index.items()
    }

    // UNWIRED: `ObjectListPanel` (`standard-resource-panels` 1.3) is the first
    // caller; until then only tests use it.
    #[allow(dead_code)]
    /// Why the kind can't be listed, if the server refused it. A panel shows this
    /// in place of an empty table.
    pub fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// Records the server's refusal to list this kind; the watch has stopped.
    pub fn set_refused(&mut self, message: String) {
        self.refused = Some(message);
    }

    /// Applies one watch event. `Apply`/`InitApply` upsert by uid and `Delete` removes
    /// by uid. `Init` starts a relist cycle (and clears an earlier refusal - the list
    /// is being served now); `InitDone` ends it, removing anything not re-seen.
    pub fn apply(&mut self, event: watcher::Event<DynamicObject>) {
        match event {
            watcher::Event::Apply(object) => {
                let row = ObjectRow::new(&object);
                self.index.apply_applied(row.uid.clone(), row);
            }
            watcher::Event::InitApply(object) => {
                let row = ObjectRow::new(&object);
                if let Some(seen) = &mut self.relisting {
                    seen.insert(row.uid.clone());
                }
                self.index.apply_applied(row.uid.clone(), row);
            }
            watcher::Event::Delete(object) => {
                self.index.apply_deleted(&ObjectRow::new(&object).uid);
            }
            watcher::Event::Init => {
                self.refused = None;
                self.relisting = Some(HashSet::new());
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
            }
        }
    }
}

#[cfg(test)]
mod tests;
