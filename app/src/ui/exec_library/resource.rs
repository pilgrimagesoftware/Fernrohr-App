//! Run from Library… and Save to Library… on a resource (`exec-command-library`
//! D3, D4): on a Pod, Deployment, StatefulSet, DaemonSet or Service, from its
//! row or its detail panel.
//!
//! Run lists only the entries that apply to the resource and runs the chosen
//! one there. Save opens the entry dialog prefilled from it - a pod's
//! controller's selector, or its labels without the per-revision hash labels -
//! and Save and Run runs the new entry there. What a workload selects is read
//! from objects already cached for some list; nothing is fetched to decide.

use super::chooser::{self, Choice};
use super::dialog::{self, Origin};
use super::run::{self, Place};
use super::store::ExecLibraryStore;
use crate::config::exec_library::{Entry, Target, WorkloadKind};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::label_selector;
use crate::k8s::pod_resolver::workload_kind;
use gpui_kit::*;
use k8s_openapi::api::core::v1::Pod;
use kube::core::{Expression, GroupVersionKind, Selector};

/// The labels a controller stamps per revision or per replica, never part of
/// what selects a workload's pods.
const HASH_LABELS: [&str; 4] = [
    "pod-template-hash",
    "controller-revision-hash",
    "statefulset.kubernetes.io/pod-name",
    "apps.kubernetes.io/pod-index",
];

/// The resource an action is on.
#[derive(Clone, Debug)]
pub(crate) enum Resource {
    Pod(Box<Pod>),
    Workload {
        kind: WorkloadKind,
        namespace: String,
        name: String,
        /// What it selects, when the caller has the object.
        selector: Option<Selector>,
    },
}

impl Resource {
    fn place(&self) -> Place {
        match self {
            Resource::Pod(pod) => Place::Pod(pod.clone()),
            Resource::Workload {
                kind,
                namespace,
                name,
                ..
            } => Place::Workload {
                kind: *kind,
                namespace: namespace.clone(),
                name: name.clone(),
            },
        }
    }

    fn describe(&self) -> String {
        match self {
            Resource::Pod(pod) => format!(
                "Pod {}/{}",
                pod.metadata.namespace.as_deref().unwrap_or_default(),
                pod.metadata.name.as_deref().unwrap_or_default()
            ),
            Resource::Workload {
                kind,
                namespace,
                name,
                ..
            } => format!("{} {namespace}/{name}", kind.as_str()),
        }
    }
}

/// What a cached object of `kind` named `namespace/name` in `context` says, if
/// some list has it.
fn cached(
    context: &str,
    kind: &DiscoveredKind,
    namespace: &str,
    name: &str,
    cx: &App,
) -> Option<crate::k8s::resource::kind_actions::ObjectFacts> {
    let table = ClusterRegistry::existing_kind(cx, context, kind)?;
    table
        .read(cx)
        .find(Some(namespace), name, None)
        .map(|row| row.facts.clone())
}

fn replica_sets() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("apps", "v1", "ReplicaSet"),
        plural: "replicasets".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// What the cached workload `kind namespace/name` selects.
fn cached_selector(
    context: &str,
    kind: WorkloadKind,
    namespace: &str,
    name: &str,
    cx: &App,
) -> Option<Selector> {
    cached(context, &workload_kind(kind), namespace, name, cx)?.pod_selector
}

/// Whether `pod` matches `namespace` (unset matches every one) and `selector`.
fn pod_matches(pod: &Pod, namespace: Option<&str>, selector: &str) -> bool {
    namespace.is_none_or(|namespace| pod.metadata.namespace.as_deref() == Some(namespace))
        && label_selector::parse(selector)
            .is_ok_and(|selector| label_selector::matches(&selector, pod.metadata.labels.as_ref()))
}

/// Whether `entry` applies to `resource` in `context` (D3).
pub(crate) fn applies(entry: &Entry, resource: &Resource, context: &str, cx: &App) -> bool {
    match (&entry.target, resource) {
        (Target::Any, _) => true,
        (
            Target::Selector {
                namespace,
                selector,
            },
            Resource::Pod(pod),
        ) => pod_matches(pod, namespace.as_deref(), selector),
        (
            Target::Workload {
                namespace,
                workload_kind,
                name,
            },
            Resource::Pod(pod),
        ) => {
            pod.metadata.namespace.as_deref() == Some(namespace.as_str())
                && cached_selector(context, *workload_kind, namespace, name, cx).is_some_and(
                    |selector| label_selector::matches(&selector, pod.metadata.labels.as_ref()),
                )
        }
        (
            Target::Workload {
                namespace,
                workload_kind,
                name,
            },
            Resource::Workload {
                kind,
                namespace: at,
                name: called,
                ..
            },
        ) => workload_kind == kind && namespace == at && name == called,
        // A selector entry applies to a workload when one of the workload's
        // cached pods matches it too.
        (
            Target::Selector {
                namespace,
                selector,
            },
            Resource::Workload {
                kind,
                namespace: at,
                name,
                selector: known,
            },
        ) => {
            let Some(own) = known
                .clone()
                .or_else(|| cached_selector(context, *kind, at, name, cx))
            else {
                return false;
            };
            let Some(pods) = ClusterRegistry::existing_pods(cx, context) else {
                return false;
            };
            pods.read(cx).pods().iter().any(|pod| {
                pod.metadata.namespace.as_deref() == Some(at.as_str())
                    && label_selector::matches(&own, pod.metadata.labels.as_ref())
                    && pod_matches(pod, namespace.as_deref(), selector)
            })
        }
    }
}

/// The title of the entry chooser Run from Library… opens.
pub(crate) const RUN_FROM_TITLE: &str = "Run from Library";

/// Run from Library…: the entries that apply to `resource`, to run one there.
pub(crate) fn run_from(resource: Resource, context: String, window: &mut Window, cx: &mut App) {
    let entries: Vec<Entry> = ExecLibraryStore::get(cx)
        .entries
        .iter()
        .filter(|entry| applies(entry, &resource, &context, cx))
        .cloned()
        .collect();
    if entries.is_empty() {
        let message = format!("No library entry applies to {}.", resource.describe());
        return run::notify("Nothing to run", &message, window, cx);
    }
    let choices = entries
        .iter()
        .map(|entry| Choice {
            label: entry.name.clone(),
            detail: Some(format!("{}  ·  {}", entry.command, entry.target.describe())),
        })
        .collect();
    let place = resource.place();
    chooser::open(
        RUN_FROM_TITLE,
        choices,
        move |index, window, cx| {
            run::run(
                entries[index].clone(),
                context.clone(),
                place.clone(),
                window,
                cx,
            )
        },
        window,
        cx,
    );
}

/// Save to Library…: the entry dialog, prefilled from `resource` (D4).
pub(crate) fn save_from(resource: Resource, context: String, window: &mut Window, cx: &mut App) {
    let target = prefill(&resource, &context, cx);
    let entry = Entry {
        name: String::new(),
        command: String::new(),
        container: None,
        target,
    };
    let origin = Origin::Resource {
        context,
        place: resource.place(),
    };
    dialog::open_with(Some(entry), false, origin, window, cx);
}

/// The target a new entry from `resource` starts with.
pub(crate) fn prefill(resource: &Resource, context: &str, cx: &App) -> Target {
    match resource {
        Resource::Workload {
            kind,
            namespace,
            name,
            ..
        } => Target::Workload {
            namespace: namespace.clone(),
            workload_kind: *kind,
            name: name.clone(),
        },
        Resource::Pod(pod) => {
            let namespace = pod.metadata.namespace.clone().unwrap_or_default();
            let selector = owner_selector(pod, context, cx)
                .unwrap_or_else(|| own_labels(pod))
                .to_string();
            Target::Selector {
                namespace: Some(namespace),
                selector,
            }
        }
    }
}

/// The selector of `pod`'s controlling workload - through its ReplicaSet to a
/// Deployment - when those objects are cached.
fn owner_selector(pod: &Pod, context: &str, cx: &App) -> Option<Selector> {
    let namespace = pod.metadata.namespace.as_deref()?;
    let owner = pod
        .metadata
        .owner_references
        .iter()
        .flatten()
        .find(|owner| owner.controller == Some(true))?;
    match owner.kind.as_str() {
        "ReplicaSet" => {
            let replica_set = cached(context, &replica_sets(), namespace, &owner.name, cx)?;
            match replica_set.controller {
                Some((kind, name)) if kind == "Deployment" => {
                    cached_selector(context, WorkloadKind::Deployment, namespace, &name, cx)
                }
                _ => replica_set.pod_selector,
            }
        }
        "StatefulSet" => cached_selector(
            context,
            WorkloadKind::StatefulSet,
            namespace,
            &owner.name,
            cx,
        ),
        "DaemonSet" => {
            cached_selector(context, WorkloadKind::DaemonSet, namespace, &owner.name, cx)
        }
        _ => None,
    }
}

/// `pod`'s own labels, without the per-revision hash labels.
fn own_labels(pod: &Pod) -> Selector {
    pod.metadata
        .labels
        .iter()
        .flatten()
        .filter(|(key, _)| !HASH_LABELS.contains(&key.as_str()))
        .map(|(key, value)| Expression::Equal(key.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod tests;
