//! Following the object live (`live-detail-panels` 2.1): once connected, the
//! panel subscribes to its context's shared watch of its kind - the one a list
//! of that kind already reads, polling included for kinds that can't be
//! watched - and finds its object's row by namespace and name.
//!
//! A row holds only what a list shows, not the object, so the row is the
//! change signal rather than the source: a new `resourceVersion` (or uid) on
//! the row refetches the object through the same `get` the first paint uses,
//! which also redacts it and re-lists its events. One `get` per write to this
//! one object keeps whole objects - Secrets' values among them - out of a table
//! every list of the kind shares. A row gone from a synced table is the
//! object's absence, as for pods.
//!
//! The view state - which view, revealed values, YAML folds and scroll - is the
//! panel's, beside `state`, so a refetch re-renders without resetting it. Only a
//! recreated object (same name, new uid) hides values revealed from the old one.

use super::ObjectDetailPanel;
use super::fetch::ObjectDetailState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_list::ObjectsTable;
use gpui_kit::*;

/// Where the panel learns that its object changed.
pub(super) enum LiveSource {
    /// Never follows the cluster: a test panel over a stub connection, which has
    /// no session to subscribe through. It shows its one fetch.
    #[cfg(test)]
    Off,
    /// Subscribes once the connection is up.
    Pending,
    /// Subscribed to the kind's shared watch, and released with the panel.
    Following(Entity<ObjectsTable>),
}

/// The write of the object a table row stands for: its uid and `resourceVersion`.
#[derive(Clone, PartialEq)]
pub(super) struct Version {
    uid: String,
    resource_version: String,
}

impl ObjectDetailPanel {
    /// Subscribes to the kind's shared watch with `client`, unless the panel
    /// already does or never follows.
    pub(super) fn follow_kind(&mut self, client: kube::Client, cx: &mut Context<Self>) {
        if !matches!(self.live, LiveSource::Pending) {
            return;
        }
        let context_name = self.scope.context_name.clone();
        let kind = self.target.kind.clone();
        let table = ClusterRegistry::subscribe_kind(cx, &context_name, client, &kind);
        cx.on_release(move |_: &mut Self, cx| {
            ClusterRegistry::unsubscribe_kind(cx, &context_name, &kind)
        })
        .detach();
        cx.observe(&table, |this, _, cx| this.read_from_table(cx))
            .detach();
        self.live = LiveSource::Following(table);
        self.read_from_table(cx);
    }

    /// Acts on what the table says about the object: refetches it when its row
    /// shows a write the panel hasn't fetched, and shows it gone once the row is.
    pub(super) fn read_from_table(&mut self, cx: &mut Context<Self>) {
        let LiveSource::Following(table) = &self.live else {
            return;
        };
        let row = {
            let table = table.read(cx);
            if !table.synced() {
                return;
            }
            let followed_uid = self.followed.as_ref().map(|seen| seen.uid.as_str());
            table
                .find(
                    self.target.namespace.as_deref(),
                    &self.target.name,
                    followed_uid,
                )
                .map(|row| Version {
                    uid: row.uid.clone(),
                    resource_version: row.resource_version.clone(),
                })
        };
        let Some(row) = row else {
            self.followed = None;
            if !matches!(self.state, ObjectDetailState::NotFound) {
                self.state = ObjectDetailState::NotFound;
                cx.notify();
            }
            return;
        };
        if self.followed.as_ref() == Some(&row) {
            return;
        }
        let shown = self.object().map(|object| Version {
            uid: object.metadata.uid.clone().unwrap_or_default(),
            resource_version: object.metadata.resource_version.clone().unwrap_or_default(),
        });
        let recreated = self
            .followed
            .as_ref()
            .or(shown.as_ref())
            .is_some_and(|before| before.uid != row.uid);
        if recreated {
            self.revealed.clear();
        }
        let current = shown.as_ref() == Some(&row);
        self.followed = Some(row);
        if !current {
            self.fetch(cx);
        }
    }
}

#[cfg(test)]
impl ObjectDetailPanel {
    /// The table the panel follows, for tests to drive.
    pub(super) fn live_table(&self) -> Option<Entity<ObjectsTable>> {
        match &self.live {
            LiveSource::Following(table) => Some(table.clone()),
            LiveSource::Off | LiveSource::Pending => None,
        }
    }
}
