//! Which kinds each connected cluster reports, for any panel that needs to ask.
//!
//! The Resource panel runs its own discovery for its list; this registry holds
//! one shared result per context for everything else - chiefly
//! `ui::nav::viewer_for`, which needs to know whether a referenced kind exists
//! in the referencing panel's cluster before it shows the reference as a link.
//! Built the same way `NamespaceRegistry` is: an entity per context, created on
//! first use and loaded once that context connects.
//!
//! The Resource panel [`publish`](DiscoveryRegistry::publish)es each of its own
//! results here too - the first and every Refresh - so a group that recovers
//! becomes linkable the moment it lists, rather than only after a reconnect.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::{DiscoveredKind, discover_kinds};
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::*;
use std::collections::HashMap;

#[derive(Default)]
pub struct DiscoveryRegistry(HashMap<String, Entity<DiscoveredKinds>>);

impl Global for DiscoveryRegistry {}

impl DiscoveryRegistry {
    /// `context_name`'s discovery, starting it if nothing has asked yet.
    pub fn kinds(cx: &mut App, context_name: &str) -> Entity<DiscoveredKinds> {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        if let Some(kinds) = cx.global::<Self>().0.get(context_name) {
            return kinds.clone();
        }
        let context_name = context_name.to_string();
        let kinds = cx.new(|cx| DiscoveredKinds::new(&context_name, cx));
        cx.global_mut::<Self>()
            .0
            .insert(context_name, kinds.clone());
        kinds
    }

    /// Makes `kinds` `context_name`'s discovery: the Resource panel's latest result,
    /// newer than any this registry ran itself.
    pub fn publish(cx: &mut App, context_name: &str, kinds: Vec<DiscoveredKind>) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        match cx.global::<Self>().0.get(context_name).cloned() {
            Some(entry) => entry.update(cx, |entry, cx| {
                entry.kinds = Some(kinds);
                cx.notify();
            }),
            None => {
                let entry = cx.new(|_| DiscoveredKinds {
                    kinds: Some(kinds),
                    loading: false,
                });
                cx.global_mut::<Self>()
                    .0
                    .insert(context_name.to_string(), entry);
            }
        }
    }

    /// Seeds `context_name` with a fixed result, so a test never starts a real
    /// connect - see `DiscoveredKinds::loaded`.
    #[cfg(test)]
    pub(crate) fn insert_test(cx: &mut App, context_name: &str, kinds: Vec<DiscoveredKind>) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        let kinds = cx.new(|_| DiscoveredKinds::loaded(kinds));
        cx.global_mut::<Self>()
            .0
            .insert(context_name.to_string(), kinds);
    }
}

/// One context's discovered kinds: `None` until discovery has succeeded.
pub struct DiscoveredKinds {
    kinds: Option<Vec<DiscoveredKind>>,
    loading: bool,
}

impl DiscoveredKinds {
    fn new(context_name: &str, cx: &mut Context<Self>) -> Self {
        let connection = ClusterRegistry::connection(cx, context_name);
        cx.observe(&connection, |this: &mut Self, connection, cx| {
            this.sync(&connection, cx);
        })
        .detach();
        let mut this = Self {
            kinds: None,
            loading: false,
        };
        this.sync(&connection, cx);
        this
    }

    /// A result that is already in and never syncs, for tests.
    #[cfg(test)]
    pub(crate) fn loaded(kinds: Vec<DiscoveredKind>) -> Self {
        Self {
            kinds: Some(kinds),
            loading: false,
        }
    }

    /// The kinds, once discovery has succeeded.
    pub fn kinds(&self) -> Option<&[DiscoveredKind]> {
        self.kinds.as_deref()
    }

    /// Runs discovery once the context is connected. A failure leaves `kinds`
    /// unset, so the next connection change (a reconnect) tries again rather
    /// than the context being stuck with nothing linkable.
    fn sync(&mut self, connection: &Entity<ClusterConnection>, cx: &mut Context<Self>) {
        if self.loading || self.kinds.is_some() {
            return;
        }
        let ConnectionState::Connected(client) = &connection.read(cx).state else {
            return;
        };
        self.loading = true;
        let client = client.clone();
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx.send(discover_kinds(client).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.loading = false;
                    match result {
                        Ok(discovered) => {
                            for group in &discovered.unavailable {
                                log::warn!(
                                    "API group {:?} unavailable: {}",
                                    group.group,
                                    group.reason
                                );
                            }
                            // A result the Resource panel published meanwhile is
                            // newer than this run's; keep it.
                            if this.kinds.is_none() {
                                this.kinds = Some(discovered.kinds);
                            }
                        }
                        Err(error) => log::warn!("resource discovery failed: {error}"),
                    }
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }
}
