//! The live set of one kind's objects that one watch keeps up to date, including the
//! relist sweep, and the refusal a forbidden kind shows in place of rows.

use super::columns::{self, KindColumns};
use super::row::ObjectRow;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::util::resource_index::ResourceIndex;
use kube::api::DynamicObject;
use kube_runtime::watcher;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Notify;

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
    /// The kind's own columns, whose cells each row is built with - `None` for
    /// a kind with base columns only.
    columns: Option<&'static KindColumns>,
    /// How the rows are kept current: a watch, polling, or not at all.
    mode: ListMode,
}

/// How a kind's rows are kept current (`unwatchable-kinds`).
#[derive(Clone, Debug, Default)]
pub enum ListMode {
    /// A `kube_runtime` watch, the normal case.
    #[default]
    Watched,
    /// Re-listed every `consts::LIST_POLL_INTERVAL`, for a kind that can be
    /// listed but not watched. `refresh` wakes the poller for a re-list now.
    Polled { refresh: Arc<Notify> },
    /// Discovery reports no `list` verb, so there are no rows to show.
    Unlistable,
}

impl ObjectsTable {
    /// A table for `kind`'s objects, building each row with the kind's own
    /// columns' cells.
    pub fn for_kind(kind: &DiscoveredKind) -> Self {
        Self {
            columns: columns::for_kind(&kind.gvk.group, &kind.gvk.kind),
            ..Self::default()
        }
    }

    pub fn rows(&self) -> &[ObjectRow] {
        self.index.items()
    }

    /// Why the kind can't be listed, if the server refused it. A panel shows this
    /// in place of an empty table.
    pub fn refused(&self) -> Option<&str> {
        self.refused.as_deref()
    }

    /// Records the server's refusal to list this kind; the watch has stopped.
    pub fn set_refused(&mut self, message: String) {
        self.refused = Some(message);
    }

    /// How the rows are kept current.
    pub fn mode(&self) -> &ListMode {
        &self.mode
    }

    pub fn set_mode(&mut self, mode: ListMode) {
        self.mode = mode;
    }

    /// Asks a polled table's poller to re-list now. Nothing for a watched or
    /// unlistable one - a watch is already current.
    pub fn request_refresh(&self) {
        if let ListMode::Polled { refresh } = &self.mode {
            refresh.notify_one();
        }
    }

    /// Replaces every row with `objects` - one poll's list, applied as a
    /// watch's relist is, so an object gone from the list goes from the table.
    pub fn replace_all(&mut self, objects: Vec<DynamicObject>) {
        self.apply(watcher::Event::Init);
        for object in objects {
            self.apply(watcher::Event::InitApply(object));
        }
        self.apply(watcher::Event::InitDone);
    }

    /// Applies one watch event. `Apply`/`InitApply` upsert by uid and `Delete` removes
    /// by uid. `Init` starts a relist cycle (and clears an earlier refusal - the list
    /// is being served now); `InitDone` ends it, removing anything not re-seen.
    pub fn apply(&mut self, event: watcher::Event<DynamicObject>) {
        match event {
            watcher::Event::Apply(object) => {
                let row = ObjectRow::new(&object, self.columns);
                self.index.apply_applied(row.uid.clone(), row);
            }
            watcher::Event::InitApply(object) => {
                let row = ObjectRow::new(&object, self.columns);
                if let Some(seen) = &mut self.relisting {
                    seen.insert(row.uid.clone());
                }
                self.index.apply_applied(row.uid.clone(), row);
            }
            watcher::Event::Delete(object) => {
                self.index
                    .apply_deleted(object.metadata.uid.as_deref().unwrap_or_default());
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
