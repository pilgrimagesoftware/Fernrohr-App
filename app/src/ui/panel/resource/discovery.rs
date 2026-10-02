//! Running API discovery for the panel (`discovery-resilience`): the first run once
//! the context connects, and a refresh on request - the header's button or
//! Resources: Refresh (`cmd-r`). A refresh keeps the rows, the selection and every
//! collapse state while it runs, and replaces only the kinds and the unavailable
//! groups when it lands. A group that failed is kept as a non-fatal note
//! ([`Notes`]); only failing to list the groups at all is the panel's error state.

use super::{ResourcePanel, ResourceState};
use crate::command::{Command, CommandRegistry, MenuSlot, ViewGroup};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::{UnavailableGroup, discover_kinds};
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::*;

actions!(resource_panel, [RefreshResources]);

pub(crate) const REFRESH_COMMAND_ID: &str = "resource.refresh";
pub(crate) const REFRESH_DEFAULT_BINDING: &str = "cmd-r";

/// What the last discovery left besides the kinds: the API groups it couldn't
/// read, and whether their list is expanded under the warning row.
#[derive(Default)]
pub(super) struct Notes {
    pub(super) unavailable: Vec<UnavailableGroup>,
    pub(super) expanded: bool,
}

/// A global command, so the palette and the View menu reach it wherever focus is;
/// the window hands it to its Resource panel.
pub(super) fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: REFRESH_COMMAND_ID,
        title: "Resources: Refresh",
        default_binding: REFRESH_DEFAULT_BINDING,
        context: None,
        action: Box::new(RefreshResources),
        menu: Some(MenuSlot::View(ViewGroup::ResourcePanel)),
    });
}

impl ResourcePanel {
    /// Starts discovery as soon as the window's context has a client. A no-op
    /// while the state is already settled or a request is in flight.
    pub(super) fn sync(&mut self, connection: &Entity<ClusterConnection>, cx: &mut Context<Self>) {
        if self.loading || matches!(self.state, ResourceState::Loaded(_)) {
            return;
        }
        let ConnectionState::Connected(client) = &connection.read(cx).state else {
            return;
        };
        self.run_discovery(client.clone(), cx);
    }

    /// Re-runs discovery for the selected context, keeping what's on screen until
    /// the result lands. A no-op while one is already running or before the
    /// context has connected.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        let connection = ClusterRegistry::connection(cx, &self.context_name);
        let ConnectionState::Connected(client) = &connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        self.run_discovery(client, cx);
    }

    /// Whether a discovery is running - the header's refresh button spins.
    pub(super) fn discovering(&self) -> bool {
        self.loading
    }

    fn run_discovery(&mut self, client: kube::Client, cx: &mut Context<Self>) {
        self.loading = true;
        if !matches!(self.state, ResourceState::Loaded(_)) {
            self.state = ResourceState::Loading;
        }
        cx.notify();
        let rx = crate::runtime::spawn_stream(cx, 4, move |tx| async move {
            let _ = tx.send(discover_kinds(client).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.loading = false;
                    match result {
                        Ok(discovered) => {
                            // Link-following reads the same kinds the list shows.
                            crate::k8s::cluster::discovery_registry::DiscoveryRegistry::publish(
                                cx,
                                &this.context_name,
                                discovered.kinds.clone(),
                            );
                            this.state = ResourceState::Loaded(discovered.kinds);
                            this.notes.unavailable = discovered.unavailable;
                        }
                        Err(failure) => {
                            this.state = ResourceState::Failed {
                                message: failure.message,
                                detail: failure.detail,
                            };
                            this.notes.unavailable.clear();
                        }
                    }
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    /// The groups the last discovery couldn't read. Test-only.
    #[cfg(test)]
    pub(crate) fn test_unavailable_groups(&self) -> Vec<String> {
        self.notes
            .unavailable
            .iter()
            .map(|group| group.group.clone())
            .collect()
    }

    /// The failure message, when discovery failed outright. Test-only.
    #[cfg(test)]
    pub(crate) fn test_failure(&self) -> Option<String> {
        match &self.state {
            ResourceState::Failed { message, .. } => Some(message.clone()),
            _ => None,
        }
    }

    /// Collapses or expands the section `kind` is in, as Left/Right do. Test-only.
    #[cfg(test)]
    pub(crate) fn test_toggle_section_of(
        &mut self,
        kind: &crate::k8s::cluster::discovery::DiscoveredKind,
        cx: &mut Context<Self>,
    ) {
        let category = super::category::Category::for_gvk(&kind.gvk.group, &kind.plural);
        self.toggle_section(category, cx);
    }

    pub(super) fn toggle_unavailable(&mut self, cx: &mut Context<Self>) {
        self.notes.expanded = !self.notes.expanded;
        cx.notify();
    }
}
