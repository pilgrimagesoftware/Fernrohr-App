//! The Pods panel's per-namespace fallback (#457), as the generic list panel's
//! (`object_list/panel/fallback.rs`): when the server refuses the all-namespaces Pods
//! list - an account whose RBAC only allows some namespaces - a panel scoped to particular
//! namespaces watches each of them on its own ([`PodsFollow`], `WatchKey::PodsIn`) and
//! shows them as one table.
//!
//! The panel keeps holding the refused all-namespaces watch, so a restart the server allows
//! ends the fallback. A Node's embedded table spans every namespace by design, so it has
//! nothing to fall back to.
//!
//! Everything the panel reads from its pods goes through the `listed_*` methods here.

use super::*;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::PodsFollow;
use crate::k8s::resource::load_phase::LoadPhase;
use crate::ui::row_flash::{ChangeJournal, Changes};

/// The namespaces a Pods panel watches one by one while its all-namespaces list is refused.
#[derive(Default)]
pub(super) struct PodsFallback {
    /// One watch per namespace, in the scope's order. Empty while the all-namespaces
    /// watch serves the panel.
    watches: Vec<NamespacePods>,
}

struct NamespacePods {
    namespace: String,
    follow: PodsFollow,
    _observe: Subscription,
}

impl PodsFallback {
    /// The namespaces watched now, in order.
    #[cfg(test)]
    pub(super) fn namespaces(&self) -> Vec<String> {
        self.watches
            .iter()
            .map(|watch| watch.namespace.clone())
            .collect()
    }

    /// Releases every namespace's watch, as the panel closing does.
    pub(super) fn release_all(&mut self, cx: &mut App) {
        for watch in self.watches.drain(..) {
            watch.follow.release(cx);
        }
    }
}

impl PodsPanel {
    /// Starts or stops the per-namespace watches to match the panel now: one per scoped
    /// namespace while the all-namespaces list is refused, none otherwise. Call when the
    /// all-namespaces table changes and when the scope does.
    pub(super) fn sync_fallback(&mut self, cx: &mut Context<Self>) {
        let refused =
            self.subscribed && self.node.is_none() && self.table.read(cx).refused().is_some();
        let wanted: &[String] = if refused { &self.scope.namespaces } else { &[] };
        let mut released = Vec::new();
        let mut kept = Vec::new();
        for watch in self.fallback.watches.drain(..) {
            if wanted.contains(&watch.namespace) {
                kept.push(watch);
            } else {
                released.push(watch);
            }
        }
        self.fallback.watches = kept;
        for watch in released {
            watch.follow.release(cx);
        }
        let missing: Vec<String> = wanted
            .iter()
            .filter(|namespace| {
                !self
                    .fallback
                    .watches
                    .iter()
                    .any(|watch| &watch.namespace == *namespace)
            })
            .cloned()
            .collect();
        if missing.is_empty() {
            return;
        }
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        for namespace in missing {
            let follow = PodsFollow::subscribe(
                cx,
                &self.scope.context_name,
                client.clone(),
                Some(&namespace),
            );
            let observe = cx.observe(follow.table(), |_, _, cx| cx.notify());
            self.fallback.watches.push(NamespacePods {
                namespace,
                follow,
                _observe: observe,
            });
        }
        let order = &self.scope.namespaces;
        self.fallback.watches.sort_by_key(|watch| {
            order
                .iter()
                .position(|namespace| *namespace == watch.namespace)
        });
        cx.notify();
    }

    /// The tables the panel's pods come from now.
    fn listed_tables(&self) -> Vec<&Entity<PodsTable>> {
        if self.fallback.watches.is_empty() {
            vec![&self.table]
        } else {
            self.fallback
                .watches
                .iter()
                .map(|watch| watch.follow.table())
                .collect()
        }
    }

    /// The table `namespace`'s pods are read from: its own while the fallback watches it,
    /// else the all-namespaces one.
    pub(super) fn table_for(&self, namespace: &str) -> &Entity<PodsTable> {
        self.fallback
            .watches
            .iter()
            .find(|watch| watch.namespace == namespace)
            .map_or(&self.table, |watch| watch.follow.table())
    }

    /// Every listed pod, before the panel's namespace scope and filter.
    pub(super) fn listed_pods<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = &'a Pod> + 'a {
        self.listed_tables()
            .into_iter()
            .flat_map(move |table| table.read(cx).pods())
    }

    /// The pod named `name` in `namespace`, as listed.
    pub(super) fn find_listed<'a>(
        &'a self,
        namespace: &str,
        name: &str,
        cx: &'a App,
    ) -> Option<&'a Pod> {
        self.table_for(namespace)
            .read(cx)
            .find(namespace, name, None)
    }

    /// Why the panel can't list pods: the all-namespaces refusal while no fallback runs,
    /// or - while one does - only when every namespace is refused too.
    pub(super) fn listed_refusal(&self, cx: &App) -> Option<String> {
        let tables = self.listed_tables();
        let mut refusals = tables.iter().map(|table| table.read(cx).refused());
        let first = refusals.next().flatten()?;
        refusals
            .all(|refused| refused.is_some())
            .then(|| first.to_string())
    }

    /// What a refused panel says. One scoped to every namespace says how to list the
    /// namespaces it may read instead.
    pub(super) fn refusal_message(&self) -> String {
        if self.node.is_none() && self.scope.namespaces.is_empty() {
            "Can't list pods in every namespace. Choose the namespaces you can read in the \
             namespace picker to list those."
                .to_string()
        } else {
            "Can't list pods".to_string()
        }
    }

    /// Where the listed tables are in their lists, as one.
    pub(super) fn listed_phase(&self, cx: &App) -> LoadPhase {
        LoadPhase::combined(
            self.listed_tables()
                .into_iter()
                .map(|table| table.read(cx).phase()),
        )
    }

    /// The live changes since `seen` across the listed tables (#275).
    pub(super) fn listed_changes(&self, seen: Option<u64>, cx: &App) -> Changes {
        ChangeJournal::since_all(
            self.listed_tables()
                .into_iter()
                .map(|table| table.read(cx).journal()),
            seen,
        )
    }
}

#[cfg(test)]
#[path = "fallback_tests.rs"]
mod tests;
