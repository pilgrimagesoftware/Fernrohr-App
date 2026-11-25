//! A list panel's per-namespace fallback (#457): when the server refuses the cluster-wide
//! list of a namespaced kind - an account whose RBAC only allows some namespaces - the
//! panel lists each namespace it is scoped to on its own and shows them as one list.
//!
//! The panel keeps holding the refused cluster-wide watch, so a later restart that the
//! server allows (a resume, a refreshed credential) ends the fallback again. Each
//! namespace's watch is the shared one ([`ClusterRegistry::subscribe_kind_in`]), so a
//! detail panel in that namespace reads the same stream.
//!
//! Everything a list panel reads from its rows' store goes through the `listed_*` methods
//! here, which read the fallback's tables while it runs and the cluster-wide one otherwise.
//! This module does not decide how the result is drawn; that is `render`'s.

use super::ObjectListPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, KindSubscription};
use crate::k8s::resource::load_phase::LoadPhase;
use crate::k8s::resource::object_list::columns::{ConditionsSeen, PrinterColumns};
use crate::k8s::resource::object_list::row::ObjectRow;
use crate::k8s::resource::object_list::store::{ListMode, ObjectsTable};
use crate::ui::row_flash::{ChangeJournal, Changes};
use gpui_kit::{App, Context, Entity, Subscription};

/// The namespaces a list panel watches one by one while its cluster-wide list is refused.
#[derive(Default)]
pub(in crate::k8s::resource::object_list) struct NamespaceFallback {
    /// One watch per namespace, in the order the panel's scope lists them. Empty while
    /// the cluster-wide list serves the panel.
    watches: Vec<NamespaceWatch>,
}

/// One namespace's shared watch, and the observation that redraws the panel when it changes.
struct NamespaceWatch {
    namespace: String,
    subscription: KindSubscription,
    _observe: Subscription,
}

impl NamespaceFallback {
    /// The namespaces watched now, in order.
    #[cfg(test)]
    pub(in crate::k8s::resource::object_list) fn namespaces(&self) -> Vec<String> {
        self.watches
            .iter()
            .map(|watch| watch.namespace.clone())
            .collect()
    }

    /// Releases every namespace's watch, as the panel closing does.
    pub(in crate::k8s::resource::object_list) fn release_all(
        &mut self,
        context_name: &str,
        cx: &mut App,
    ) {
        for watch in self.watches.drain(..) {
            ClusterRegistry::unsubscribe_kind_in(cx, context_name, &watch.subscription.key);
        }
    }
}

impl ObjectListPanel {
    /// Starts or stops the per-namespace watches to match the panel now: one per scoped
    /// namespace while the cluster-wide list of a namespaced kind is refused, none
    /// otherwise. Call when the cluster-wide table changes and when the scope does.
    ///
    /// A panel scoped to every namespace has nothing to fall back to - it can't list the
    /// namespaces one by one without knowing which it may read - so it keeps showing the
    /// refusal.
    pub(in crate::k8s::resource::object_list) fn sync_fallback(&mut self, cx: &mut Context<Self>) {
        let refused =
            self.subscribed && self.kind.namespaced && self.objects.read(cx).refused().is_some();
        let wanted: &[String] = if refused { &self.scope.namespaces } else { &[] };
        let context_name = self.scope.context_name.clone();
        let mut released = Vec::new();
        self.fallback.watches.retain(|watch| {
            let keep = wanted.contains(&watch.namespace);
            if !keep {
                released.push(watch.subscription.key.clone());
            }
            keep
        });
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
        for key in released {
            ClusterRegistry::unsubscribe_kind_in(cx, &context_name, &key);
        }
        if missing.is_empty() {
            return;
        }
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        for namespace in missing {
            let subscription = ClusterRegistry::subscribe_kind_in(
                cx,
                &context_name,
                client.clone(),
                &self.kind,
                Some(&namespace),
            );
            let observe = cx.observe(&subscription.table, |_, _, cx| cx.notify());
            self.fallback.watches.push(NamespaceWatch {
                namespace,
                subscription,
                _observe: observe,
            });
        }
        // Keep the scope's order, so the merged list doesn't reorder as namespaces come and go.
        let order = &self.scope.namespaces;
        self.fallback.watches.sort_by_key(|watch| {
            order
                .iter()
                .position(|namespace| *namespace == watch.namespace)
        });
        cx.notify();
    }

    /// The tables the panel's rows come from now: each namespace's while the fallback
    /// runs, else the cluster-wide one.
    fn listed_tables(&self) -> Vec<&Entity<ObjectsTable>> {
        if self.fallback.watches.is_empty() {
            vec![&self.objects]
        } else {
            self.fallback
                .watches
                .iter()
                .map(|watch| &watch.subscription.table)
                .collect()
        }
    }

    /// Every listed row, before the panel's namespace scope and filter.
    pub(in crate::k8s::resource::object_list) fn listed_rows<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl Iterator<Item = &'a ObjectRow> + 'a {
        self.listed_tables()
            .into_iter()
            .flat_map(move |table| table.read(cx).rows())
    }

    /// Why the panel can't list its kind: the cluster-wide refusal while no fallback
    /// runs, or - while one does - only when every namespace is refused too.
    pub(in crate::k8s::resource::object_list) fn listed_refusal(&self, cx: &App) -> Option<String> {
        let tables = self.listed_tables();
        let mut refusals = tables.iter().map(|table| table.read(cx).refused());
        let first = refusals.next().flatten()?;
        refusals
            .all(|refused| refused.is_some())
            .then(|| first.to_string())
    }

    /// What a refused panel says. One scoped to every namespace says how to list the
    /// namespaces it may read instead, since only a choice of namespaces can start the
    /// fallback.
    pub(in crate::k8s::resource::object_list) fn refusal_message(&self) -> String {
        let label = self.scope.target.list_label();
        if self.kind.namespaced && self.scope.namespaces.is_empty() {
            format!(
                "Can't list {label} in every namespace. Choose the namespaces you can read in \
                 the namespace picker to list those."
            )
        } else {
            format!("Can't list {label}")
        }
    }

    /// How the rows are kept current. Every table lists the same kind, so they share
    /// a mode; the first one's stands for all.
    pub(in crate::k8s::resource::object_list) fn listed_mode(&self, cx: &App) -> ListMode {
        self.listed_tables()
            .first()
            .map(|table| table.read(cx).mode().clone())
            .unwrap_or_default()
    }

    /// Asks every polled table to re-list now.
    pub(in crate::k8s::resource::object_list) fn request_listed_refresh(&self, cx: &App) {
        for table in self.listed_tables() {
            table.read(cx).request_refresh();
        }
    }

    /// Where the listed tables are in their lists, as one: still loading while any is on
    /// its first list, refreshing while any relists, else loaded.
    pub(in crate::k8s::resource::object_list) fn listed_phase(&self, cx: &App) -> LoadPhase {
        LoadPhase::combined(
            self.listed_tables()
                .into_iter()
                .map(|table| table.read(cx).phase()),
        )
    }

    /// What the listed objects have reported in `status.conditions`, between them.
    pub(in crate::k8s::resource::object_list) fn listed_conditions(
        &self,
        cx: &App,
    ) -> ConditionsSeen {
        self.listed_tables()
            .into_iter()
            .fold(ConditionsSeen::default(), |seen, table| {
                seen.or(table.read(cx).conditions_seen())
            })
    }

    /// The CRD printer columns the rows are built with, if the kind has them.
    pub(in crate::k8s::resource::object_list) fn listed_printer_columns<'a>(
        &'a self,
        cx: &'a App,
    ) -> Option<&'a PrinterColumns> {
        self.listed_tables()
            .into_iter()
            .find_map(|table| table.read(cx).printer_columns())
    }

    /// The live changes since `seen` across the listed tables (#275).
    pub(in crate::k8s::resource::object_list) fn listed_changes(
        &self,
        seen: Option<u64>,
        cx: &App,
    ) -> Changes {
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
