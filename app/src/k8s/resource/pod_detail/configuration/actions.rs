//! The panel's side of the Configuration tab: loading the cards when the tab
//! is first shown, revealing one Secret value, hiding them all, and expanding
//! or collapsing one large ConfigMap value.

use super::entries::{ConfigEntry, entries};
use super::fetch::fetch_card;
use super::state::{CardContents, Reveal};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::pod_detail::commands::{HideSecretValues, SelectConfigurationTab};
use crate::k8s::resource::pod_detail::model::DetailSection;
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::secret_value::reveal;
use gpui_kit::*;

impl PodDetailPanel {
    /// The tab's entries, from the loaded pod - empty until it loads.
    pub(in crate::k8s::resource::pod_detail) fn configuration_entries(&self) -> Vec<ConfigEntry> {
        self.pod().map(entries).unwrap_or_default()
    }

    pub(in crate::k8s::resource::pod_detail) fn client(&self, cx: &App) -> Option<kube::Client> {
        match &self.connection.read(cx).state {
            ConnectionState::Connected(client) => Some(client.clone()),
            _ => None,
        }
    }

    /// Requests every card's object, once per panel. A no-op before the pod
    /// has loaded or the cluster is connected; the pod's arrival calls this
    /// again while the tab is showing.
    pub(in crate::k8s::resource::pod_detail) fn ensure_configuration_loaded(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.configuration.requested || self.pod().is_none() {
            return;
        }
        let Some(client) = self.client(cx) else {
            return;
        };
        self.configuration.requested = true;
        for entry in self.configuration_entries() {
            let target = entry.target;
            self.configuration
                .cards
                .insert(target.clone(), CardContents::Loading);
            let client = client.clone();
            let fetched = target.clone();
            let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
                let _ = tx.send(fetch_card(client, fetched).await).await;
            });
            cx.spawn(async move |this, cx| {
                crate::runtime::drain(rx, |contents| {
                    let _ = this.update(cx, |this, cx| {
                        this.configuration.cards.insert(target.clone(), contents);
                        cx.notify();
                    });
                })
                .await;
            })
            .detach();
        }
    }

    /// Reveals `key` of `secret` with a fresh read of that one value, or hides
    /// it if it's already revealed. A reveal that lands after its value was
    /// hidden again is dropped.
    pub(in crate::k8s::resource::pod_detail) fn toggle_reveal(
        &mut self,
        secret: ObjectRef,
        key: String,
        cx: &mut Context<Self>,
    ) {
        let slot = (secret.clone(), key.clone());
        if self.configuration.revealed.remove(&slot).is_some() {
            cx.notify();
            return;
        }
        let Some(client) = self.client(cx) else {
            return;
        };
        self.configuration
            .revealed
            .insert(slot.clone(), Reveal::Pending);
        let namespace = secret.namespace.clone().unwrap_or_default();
        let name = secret.name.clone();
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx.send(reveal(client, namespace, name, key).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    if let Some(entry) = this.configuration.revealed.get_mut(&slot)
                        && matches!(entry, Reveal::Pending)
                    {
                        *entry = match result {
                            Ok(value) => Reveal::Shown(value),
                            Err(error) => Reveal::Failed(error),
                        };
                        cx.notify();
                    }
                });
            })
            .await;
        })
        .detach();
        cx.notify();
    }

    /// Expands ConfigMap `target`'s `key` to its full value, or collapses it
    /// again.
    pub(in crate::k8s::resource::pod_detail) fn toggle_expanded(
        &mut self,
        target: ObjectRef,
        key: String,
        cx: &mut Context<Self>,
    ) {
        let slot = (target, key);
        if !self.configuration.expanded.remove(&slot) {
            self.configuration.expanded.insert(slot);
        }
        cx.notify();
    }

    pub(in crate::k8s::resource::pod_detail) fn on_action_select_configuration_tab(
        &mut self,
        _: &SelectConfigurationTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_tab(DetailSection::Configuration, window, cx);
    }

    pub(in crate::k8s::resource::pod_detail) fn on_action_hide_secret_values(
        &mut self,
        _: &HideSecretValues,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configuration.hide_all();
        cx.notify();
    }
}
