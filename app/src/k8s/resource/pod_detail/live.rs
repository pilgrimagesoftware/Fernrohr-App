//! Following the pod live (`live-detail-panels` D1/D2): once connected, the panel
//! subscribes to its context's shared Pods watch - the one every Pods list on the
//! context already reads, so an open list costs no second stream - and reads its
//! pod from that table by namespace and name. The one-shot `get` in `panel::sync`
//! only paints the panel until the table has finished its first list; from then
//! on the table is the source, so a pod that goes Pending to Running shows it.
//!
//! What the user has open - tab, scroll, expanded rows, folded YAML - lives on
//! the panel beside `state`, so replacing the pod re-renders values without
//! resetting any of it (D3). Only a recreated pod (same name, new uid) drops the
//! Configuration tab's cards and revealed values, which belonged to the old one.

use super::PodDetailPanel;
use super::fetch::PodDetailState;
use super::model::DetailSection;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::PodsTable;
use gpui_kit::*;
use k8s_openapi::api::core::v1::Pod;

/// Where the panel reads its pod once the first paint is done.
pub(super) enum LiveSource {
    /// Never follows the cluster: a test panel over a stub connection, which has
    /// no session to subscribe through. It shows its one fetch, as panels did
    /// before this change.
    #[cfg(test)]
    Off,
    /// Subscribes once the connection is up.
    Pending,
    /// Subscribed to the context's shared Pods watch, and released with the panel.
    Following(Entity<PodsTable>),
}

/// What the table says about the panel's pod, compared with what it shows.
enum Seen {
    /// Not listed yet, or unchanged since the panel last read it.
    Nothing,
    Absent,
    Changed(Box<Pod>),
}

impl PodDetailPanel {
    /// Subscribes to the context's shared Pods watch with `client`, unless the
    /// panel already does or never follows.
    pub(super) fn follow_pods(&mut self, client: kube::Client, cx: &mut Context<Self>) {
        if !matches!(self.live, LiveSource::Pending) {
            return;
        }
        let context_name = self.scope.context_name.clone();
        let table = ClusterRegistry::subscribe_pods(cx, &context_name, client);
        cx.on_release(move |_: &mut Self, cx| ClusterRegistry::unsubscribe_pods(cx, &context_name))
            .detach();
        cx.observe(&table, |this, _, cx| this.read_from_table(cx))
            .detach();
        self.live = LiveSource::Following(table);
        // A list already open on the context has the table synced: no need to
        // wait for its next change.
        self.read_from_table(cx);
    }

    /// Whether the shared table has taken over from the first-paint fetch.
    pub(super) fn live_synced(&self, cx: &App) -> bool {
        matches!(&self.live, LiveSource::Following(table) if table.read(cx).synced())
    }

    /// Brings `state` up to the table's copy of the pod: the pod as listed now,
    /// or `NotFound` once the table no longer has it.
    pub(super) fn read_from_table(&mut self, cx: &mut Context<Self>) {
        let LiveSource::Following(table) = &self.live else {
            return;
        };
        let seen = {
            let table = table.read(cx);
            if !table.synced() {
                Seen::Nothing
            } else {
                let shown = self.pod();
                let shown_uid = shown.and_then(|pod| pod.metadata.uid.as_deref());
                match table.find(&self.pod.namespace, &self.pod.name, shown_uid) {
                    None => match self.state {
                        PodDetailState::NotFound => Seen::Nothing,
                        _ => Seen::Absent,
                    },
                    Some(listed) if shown.is_some_and(|shown| same_version(shown, listed)) => {
                        Seen::Nothing
                    }
                    Some(listed) => Seen::Changed(Box::new(listed.clone())),
                }
            }
        };
        match seen {
            Seen::Nothing => return,
            Seen::Absent => self.state = PodDetailState::NotFound,
            Seen::Changed(pod) => self.show_listed(pod, cx),
        }
        cx.notify();
    }

    /// Shows `pod` from the table, starting its events watch if it is new to the
    /// panel and dropping what belonged to a predecessor of the same name.
    fn show_listed(&mut self, pod: Box<Pod>, cx: &mut Context<Self>) {
        let uid = pod.metadata.uid.as_deref().unwrap_or_default();
        if self.watches_other_pod(uid) {
            self.configuration = Default::default();
        }
        if let Some(client) = self.client(cx) {
            self.watch_events(&pod, client, cx);
        }
        self.state = PodDetailState::Loaded(pod);
        if self.active_tab == DetailSection::Configuration {
            self.ensure_configuration_loaded(cx);
        }
    }
}

/// Whether `shown` and `listed` are the same write of the same pod. Every write
/// bumps `resourceVersion`, so equal versions mean nothing to re-render.
fn same_version(shown: &Pod, listed: &Pod) -> bool {
    shown.metadata.uid == listed.metadata.uid
        && shown.metadata.resource_version == listed.metadata.resource_version
}
