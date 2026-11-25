//! #209: the objects a detail panel's object controls - those whose
//! `ownerReferences` name its uid - shown as the Overview's "Controls" field,
//! the reverse of its "Controlled By" (`metadata`). One level per panel:
//! each entry is a link, and the panel it opens lists the next level, so a
//! Deployment walks to its ReplicaSets and on to their Pods.
//!
//! Where the children come from: the context's shared watches, never a list
//! of its own. A kind with known children ([`CHILD_KINDS`]) subscribes their
//! watches for as long as the panel is open - the same refcounted watch a
//! list of that kind reads, so an open Pods list and this panel share one.
//! Any other kind's children are found in the watches the context already
//! has open, without starting one: a custom controller's objects show once a
//! list of their kind is open.
//!
//! #237: a PersistentVolumeClaim's panel shows the reverse of a volume
//! reference the same way: the Pods whose `spec.volumes[]
//! .persistentVolumeClaim.claimName` names it, in its namespace, as the
//! Overview's "Used By" field. It reads the context's shared Pods watch -
//! the one a Pods list or a controller's panel reads - and only re-matches
//! when that table or the object changes.
//!
//! The match runs when one of those tables or the object changes, not per
//! frame, and keeps only the references (`Controlled::refs`) for `sections`.

use super::panel::ObjectDetailPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::{ClusterRegistry, NamespaceFollow};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_list::{ObjectRow, ObjectsTable};
use crate::k8s::resource::pods::PodsTable;
use gpui_kit::*;
use k8s_openapi::api::core::v1::Pod;

/// The kinds each controller kind creates, as `(group, kind)` - whose watches
/// a panel over one of them subscribes. Pods come from the context's Pods
/// watch, the rest from their kind's.
/// A kind by API group (empty for core) and name.
type GroupKind = (&'static str, &'static str);

const CHILD_KINDS: &[(GroupKind, &[GroupKind])] = &[
    (("apps", "Deployment"), &[("apps", "ReplicaSet")]),
    (("apps", "ReplicaSet"), &[("", "Pod")]),
    (("apps", "StatefulSet"), &[("", "Pod")]),
    (("apps", "DaemonSet"), &[("", "Pod")]),
    (("batch", "CronJob"), &[("batch", "Job")]),
    (("batch", "Job"), &[("", "Pod")]),
];

/// The child kinds `(group, kind)` subscribes, or none for a kind not in
/// [`CHILD_KINDS`].
pub(super) fn child_kinds(group: &str, kind: &str) -> &'static [GroupKind] {
    CHILD_KINDS
        .iter()
        .find(|((g, k), _)| *g == group && *k == kind)
        .map(|(_, children)| *children)
        .unwrap_or_default()
}

/// The panel's controlled-objects state: the watches it subscribed and what
/// the last match found.
#[derive(Default)]
pub(super) struct Controlled {
    /// The context's Pods watch, once subscribed for a kind whose children
    /// are pods.
    pods: Option<Entity<PodsTable>>,
    /// The child kinds' watches it subscribed, each released with the panel. Each moves
    /// to a watch of the object's namespace if a shared cluster-wide one is refused (#457).
    kinds: Vec<(DiscoveredKind, NamespaceFollow)>,
    /// The context's other open watches it matched against, observed so a
    /// change there re-matches too. Only observed, never subscribed: a list
    /// closing still stops its watch, and its entry goes at the next match.
    observed: Vec<(Entity<ObjectsTable>, Subscription)>,
    /// What the object controls, sorted by kind then namespace and name.
    pub(super) refs: Vec<ObjectRef>,
    /// The plural names of child kinds the server won't let this user list, so the
    /// field says so rather than reading as "controls nothing" (#446).
    pub(super) unlistable: Vec<String>,
    /// For a PersistentVolumeClaim, the Pods that mount it, sorted by name
    /// (#237). Empty for any other kind.
    pub(super) used_by: Vec<ObjectRef>,
    /// Whether a claim's Pods watch was refused, so "Used By" says it can't
    /// list pods rather than reading as "no pod mounts it".
    pub(super) used_by_refused: bool,
}

/// Whether `group`/`kind` is a PersistentVolumeClaim, whose panel lists the
/// Pods that mount it.
pub(super) fn is_claim(group: &str, kind: &str) -> bool {
    group.is_empty() && kind == "PersistentVolumeClaim"
}

/// The Pods in `namespace` that mount the claim `claim`, sorted by name, each
/// once: through `spec.volumes[].persistentVolumeClaim.claimName`, or through a
/// generic ephemeral volume, whose claim Kubernetes names `<pod>-<volume>`.
pub(super) fn pods_using_claim(namespace: &str, claim: &str, pods: &[Pod]) -> Vec<ObjectRef> {
    let mut refs: Vec<ObjectRef> = pods
        .iter()
        .filter(|pod| pod.metadata.namespace.as_deref().unwrap_or_default() == namespace)
        .filter(|pod| {
            let pod_name = pod.metadata.name.as_deref().unwrap_or_default();
            pod.spec
                .iter()
                .flat_map(|spec| spec.volumes.iter().flatten())
                .any(|volume| {
                    volume
                        .persistent_volume_claim
                        .as_ref()
                        .is_some_and(|source| source.claim_name == claim)
                        || (volume.ephemeral.is_some()
                            && format!("{pod_name}-{}", volume.name) == claim)
                })
        })
        .map(|pod| {
            ObjectRef::core(
                "Pod",
                namespace,
                pod.metadata.name.as_deref().unwrap_or_default(),
            )
        })
        .collect();
    refs.sort_by(|a, b| a.name.cmp(&b.name));
    refs.dedup();
    refs
}

/// The plural names of the kinds among `tables` that can't be listed: refused by the server
/// (a 403) or reported by discovery as having no `list` verb. Sorted, each once.
pub(super) fn unlistable_kinds<'a>(
    tables: impl IntoIterator<Item = (&'a DiscoveredKind, &'a ObjectsTable)>,
) -> Vec<String> {
    let mut names: Vec<String> = tables
        .into_iter()
        .filter(|(_, table)| table.unlistable())
        .map(|(kind, _)| kind.plural.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Everything in `tables` and `pods` whose owners include `uid`: sorted by
/// kind, then namespace and name, each once. An empty uid - an object not
/// loaded yet - controls nothing.
pub(super) fn controlled_by<'a>(
    uid: &str,
    tables: impl IntoIterator<Item = (&'a DiscoveredKind, &'a [ObjectRow])>,
    pods: &[Pod],
) -> Vec<ObjectRef> {
    if uid.is_empty() {
        return Vec::new();
    }
    let mut refs: Vec<ObjectRef> = tables
        .into_iter()
        .flat_map(|(kind, rows)| {
            rows.iter()
                .filter(|row| row.owners.iter().any(|owner| owner == uid))
                .map(move |row| object_ref(kind, row))
        })
        .chain(pods.iter().filter_map(|pod| {
            let meta = &pod.metadata;
            meta.owner_references
                .iter()
                .flatten()
                .any(|owner| owner.uid == uid)
                .then(|| {
                    ObjectRef::core(
                        "Pod",
                        meta.namespace.as_deref().unwrap_or_default(),
                        meta.name.as_deref().unwrap_or_default(),
                    )
                })
        }))
        .collect();
    refs.sort_by(|a, b| (&a.kind, &a.namespace, &a.name).cmp(&(&b.kind, &b.namespace, &b.name)));
    refs.dedup();
    refs
}

fn object_ref(kind: &DiscoveredKind, row: &ObjectRow) -> ObjectRef {
    let gvk = &kind.gvk;
    match &row.namespace {
        Some(namespace) => {
            ObjectRef::namespaced(gvk.group.clone(), gvk.kind.clone(), namespace, &row.name)
        }
        None => ObjectRef::cluster_scoped(gvk.group.clone(), gvk.kind.clone(), &row.name),
    }
}

impl ObjectDetailPanel {
    /// Subscribes the watches of the object's known child kinds that aren't
    /// yet, once connected and the kinds are discovered - again on each
    /// connection or discovery change, so a kind discovered late still gets
    /// its watch. Then matches.
    pub(super) fn follow_controlled(&mut self, cx: &mut Context<Self>) {
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let gvk = &self.target.kind.gvk;
        let children = child_kinds(&gvk.group, &gvk.kind);
        // A claim's panel reads the Pods watch too, for the Pods that mount it.
        if is_claim(&gvk.group, &gvk.kind) {
            self.subscribe_pods_once(client.clone(), cx);
        }
        for &(group, kind) in children {
            if group.is_empty() && kind == "Pod" {
                self.subscribe_pods_once(client.clone(), cx);
                continue;
            }
            let subscribed = self
                .controlled
                .kinds
                .iter()
                .any(|(watched, _)| watched.gvk.group == group && watched.gvk.kind == kind);
            let discovered = self.kinds(cx).and_then(|kinds| {
                kinds
                    .iter()
                    .find(|found| found.gvk.group == group && found.gvk.kind == kind)
                    .cloned()
            });
            let (false, Some(child)) = (subscribed, discovered) else {
                continue;
            };
            self.watch_child_kind(child, client.clone(), cx);
        }
        self.match_controlled(cx);
    }

    /// Subscribes the context's Pods watch for as long as the panel is open,
    /// unless it already has.
    fn subscribe_pods_once(&mut self, client: kube::Client, cx: &mut Context<Self>) {
        if self.controlled.pods.is_some() {
            return;
        }
        let context_name = self.scope.context_name.clone();
        let table = ClusterRegistry::subscribe_pods(cx, &context_name, client);
        cx.on_release(move |_: &mut Self, cx| ClusterRegistry::unsubscribe_pods(cx, &context_name))
            .detach();
        cx.observe(&table, |this, _, cx| this.match_controlled(cx))
            .detach();
        self.controlled.pods = Some(table);
    }

    fn watch_child_kind(
        &mut self,
        kind: DiscoveredKind,
        client: kube::Client,
        cx: &mut Context<Self>,
    ) {
        let context_name = self.scope.context_name.clone();
        // Owned children live in the owner's namespace, so that is all the watch needs (#446).
        let namespace = self.target.namespace.clone();
        let follow =
            NamespaceFollow::subscribe(cx, &context_name, client, &kind, namespace.as_deref());
        if self.controlled.kinds.is_empty() {
            cx.on_release(|this: &mut Self, cx| {
                for (_, follow) in &this.controlled.kinds {
                    follow.release(cx);
                }
            })
            .detach();
        }
        cx.observe(follow.table(), |this, _, cx| this.match_controlled(cx))
            .detach();
        self.controlled.kinds.push((kind, follow));
    }

    /// Matches the object against its subscribed child watches and every
    /// other watch the context has open, and redraws if that changed.
    pub(super) fn match_controlled(&mut self, cx: &mut Context<Self>) {
        let uid = self
            .object()
            .and_then(|object| object.metadata.uid.clone())
            .unwrap_or_default();
        // A child watch whose shared cluster-wide one was refused moves to the object's
        // namespace first, and the new table is observed from then on.
        for (_, follow) in &mut self.controlled.kinds {
            if follow.reroute_if_refused(cx) {
                cx.observe(follow.table(), |this, _, cx| this.match_controlled(cx))
                    .detach();
            }
        }
        let mut tables: Vec<(DiscoveredKind, Entity<ObjectsTable>)> = self
            .controlled
            .kinds
            .iter()
            .map(|(kind, follow)| (kind.clone(), follow.table().clone()))
            .collect();
        let open = ClusterRegistry::open_kind_tables(cx, &self.scope.context_name);
        self.controlled
            .observed
            .retain(|(seen, _)| open.iter().any(|(_, still)| still == seen));
        // By table, not by kind: a kind can be open as a cluster-wide watch and as one
        // per namespace (#446), and each holds different rows.
        for (kind, table) in open {
            if tables.iter().any(|(_, watched)| *watched == table) {
                continue;
            }
            if !self
                .controlled
                .observed
                .iter()
                .any(|(seen, _)| *seen == table)
            {
                let subscription = cx.observe(&table, |this, _, cx| this.match_controlled(cx));
                self.controlled.observed.push((table.clone(), subscription));
            }
            tables.push((kind, table));
        }
        let read: Vec<(&DiscoveredKind, &[ObjectRow])> = tables
            .iter()
            .map(|(kind, table)| (kind, table.read(cx).rows()))
            .collect();
        let pods = self
            .controlled
            .pods
            .as_ref()
            .map(|table| table.read(cx).pods())
            .unwrap_or_default();
        let refs = controlled_by(&uid, read, pods);
        // Only the child kinds this panel subscribed: a refusal of an unrelated open
        // list says nothing about what this object controls.
        let mut unlistable = unlistable_kinds(
            self.controlled
                .kinds
                .iter()
                .map(|(kind, follow)| (kind, follow.table().read(cx))),
        );
        // The typed Pods watch is cluster-wide, so a namespace-only user is refused it.
        let pods_refused = self
            .controlled
            .pods
            .as_ref()
            .is_some_and(|table| table.read(cx).refused().is_some());
        let gvk = &self.target.kind.gvk;
        let claim = is_claim(&gvk.group, &gvk.kind);
        // A claim controls nothing, so its Pods watch says nothing about Controls.
        if pods_refused && child_kinds(&gvk.group, &gvk.kind).contains(&("", "Pod")) {
            unlistable.push("pods".to_string());
            unlistable.sort();
        }
        let used_by = if claim {
            pods_using_claim(
                self.target.namespace.as_deref().unwrap_or_default(),
                &self.target.name,
                pods,
            )
        } else {
            Vec::new()
        };
        let used_by_refused = claim && pods_refused;
        if refs != self.controlled.refs
            || unlistable != self.controlled.unlistable
            || used_by != self.controlled.used_by
            || used_by_refused != self.controlled.used_by_refused
        {
            self.controlled.refs = refs;
            self.controlled.unlistable = unlistable;
            self.controlled.used_by = used_by;
            self.controlled.used_by_refused = used_by_refused;
            cx.notify();
        }
    }
}

#[cfg(test)]
impl ObjectDetailPanel {
    /// Follows `pods` and `kinds` as if subscribed, for tests: a stub
    /// connection never connects.
    pub(super) fn test_watch_controlled(
        &mut self,
        pods: Option<Entity<PodsTable>>,
        kinds: Vec<(DiscoveredKind, Entity<ObjectsTable>)>,
        cx: &mut Context<Self>,
    ) {
        for (_, table) in &kinds {
            cx.observe(table, |this, _, cx| this.match_controlled(cx))
                .detach();
        }
        if let Some(table) = &pods {
            cx.observe(table, |this, _, cx| this.match_controlled(cx))
                .detach();
        }
        self.controlled.pods = pods;
        self.controlled.kinds = kinds
            .into_iter()
            .map(|(kind, table)| {
                let follow = NamespaceFollow::detached(&kind, table);
                (kind, follow)
            })
            .collect();
        self.match_controlled(cx);
    }
}
