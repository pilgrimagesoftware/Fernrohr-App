//! A Logs panel following every pod a label selector picks (#150): a
//! workload's pods, by its `spec.selector`, or whatever selector the user types
//! into the panel. Each matching container streams into the panel's one view,
//! its lines tagged with where they came from, and streams start and stop as
//! the context's shared Pods watch sees pods come and go.
//!
//! This module owns *which* containers to stream ([`LabelLogs`], [`wanted`]);
//! `follow` keeps the panel's streams in step with that, and `render` draws
//! the panel's selector bar. A single pod's logs stay `panel`'s.

use super::*;
use crate::k8s::label_selector;
use k8s_openapi::api::core::v1::{ContainerStatus, Pod};
use kube::core::Selector;
use std::collections::HashMap;

mod follow;
mod render;

/// Which pods a label-following Logs panel streams - the identity its
/// `NavTarget::LabelLogs` dedups on.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LabelLogs {
    /// A selector typed into the panel, over the panel's namespaces. One such
    /// panel per context: opening it again focuses it, to type another.
    Typed,
    /// One workload's pods (a Deployment's, say), by its selector.
    Workload(WorkloadRef),
}

/// A workload a Logs panel follows the pods of: what titles the panel, and
/// the selector - its `spec.selector`, as canonical text - that picks them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct WorkloadRef {
    /// The kind's name, `Deployment`.
    pub kind: String,
    pub namespace: String,
    pub name: String,
    pub selector: String,
}

impl LabelLogs {
    /// What the panel's title and tab name it by: the workload, or - typed -
    /// that it follows labels.
    pub fn label(&self) -> String {
        match self {
            LabelLogs::Typed => "Logs: Labels".to_string(),
            LabelLogs::Workload(workload) => format!("Logs: {} {}", workload.kind, workload.name),
        }
    }
}

/// One container instance a label-following panel streams. The pod's uid
/// makes a pod recreated under the same name - a StatefulSet's - a new pod,
/// and the restart count a restarted container a new instance; either's log
/// is streamed afresh.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct StreamKey {
    pub(super) namespace: String,
    pub(super) pod: String,
    pub(super) uid: String,
    pub(super) container: String,
    pub(super) restart_count: i32,
}

/// A container to stream, and the tag its lines carry: the pod, plus the
/// container when the pod has several.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Wanted {
    pub(super) key: StreamKey,
    pub(super) source: String,
}

/// How much the selector matched: pods, and the containers in them that have
/// logs to stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Matched {
    pub(super) pods: usize,
    pub(super) containers: usize,
}

/// The containers `selector` picks among `pods` in `namespaces` (all, when
/// empty), in a stable order, with how many pods and containers that is. A
/// container is streamable once it has started - running, or run and ended -
/// since a waiting one has no log to read yet; it's picked up on the pod's
/// next update.
pub(super) fn wanted(
    pods: &[Pod],
    selector: &Selector,
    namespaces: &[String],
) -> (Vec<Wanted>, Matched) {
    let mut matched = Matched::default();
    let mut wanted = Vec::new();
    for pod in pods {
        if !crate::k8s::resource::pods::matches_namespaces(pod, namespaces)
            || !label_selector::matches(selector, pod.metadata.labels.as_ref())
        {
            continue;
        }
        matched.pods += 1;
        let started: Vec<&ContainerStatus> = pod
            .status
            .as_ref()
            .and_then(|status| status.container_statuses.as_ref())
            .into_iter()
            .flatten()
            .filter(|container| has_started(container))
            .collect();
        let several = started.len() > 1;
        let namespace = pod.metadata.namespace.clone().unwrap_or_default();
        let name = pod.metadata.name.clone().unwrap_or_default();
        let uid = pod.metadata.uid.clone().unwrap_or_default();
        for container in started {
            matched.containers += 1;
            let source = if several {
                format!("{name}/{}", container.name)
            } else {
                name.clone()
            };
            wanted.push(Wanted {
                key: StreamKey {
                    namespace: namespace.clone(),
                    pod: name.clone(),
                    uid: uid.clone(),
                    container: container.name.clone(),
                    restart_count: container.restart_count,
                },
                source,
            });
        }
    }
    wanted.sort_by(|a, b| a.key.cmp(&b.key));
    (wanted, matched)
}

fn has_started(container: &ContainerStatus) -> bool {
    container
        .state
        .as_ref()
        .is_some_and(|state| state.running.is_some() || state.terminated.is_some())
}

/// A label-following panel's state beside the single-pod one's.
pub(super) struct Following {
    pub(super) source: LabelLogs,
    /// The selector streamed now, as canonical text and parsed; `None` while
    /// a typed panel has none yet.
    pub(super) selector: Option<(String, Selector)>,
    /// Why the selector last typed wasn't applied.
    pub(super) error: Option<String>,
    /// A typed panel's selector field, made on its first render.
    pub(super) input: Option<Entity<gpui_kit::component::input::InputState>>,
    /// The context's shared Pods watch, once the panel has subscribed to it.
    pub(super) pods: Option<Entity<crate::k8s::resource::pods::PodsTable>>,
    pub(super) streams: HashMap<StreamKey, Task<()>>,
    pub(super) matched: Matched,
}

impl Following {
    /// `source`'s state, streaming `selector` - a workload's own, or a typed
    /// panel's restored one - when there is one that parses.
    pub(super) fn new(source: LabelLogs, selector: Option<&str>) -> Self {
        let selector = match (&source, selector) {
            (LabelLogs::Workload(workload), _) => Some(workload.selector.as_str()),
            (LabelLogs::Typed, selector) => selector,
        };
        let selector = selector
            .and_then(|text| label_selector::parse(text).ok())
            .filter(|selector| !selector.selects_all())
            .map(|selector| (selector.to_string(), selector));
        Self {
            source,
            selector,
            error: None,
            input: None,
            pods: None,
            streams: HashMap::new(),
            matched: Matched::default(),
        }
    }
}

/// A saved Logs panel's label source and the selector it streamed, if it was a
/// label-following one.
pub(crate) fn labels_from_state(state: &serde_json::Value) -> Option<(LabelLogs, Option<String>)> {
    let source = serde_json::from_value(state.get("label_logs")?.clone()).ok()?;
    let selector = state["selector"].as_str().map(str::to_string);
    Some((source, selector))
}

#[cfg(test)]
mod tests;
