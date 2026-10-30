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

mod context;
mod follow;
mod render;
mod scope;

// For the window tests in `util::shell`.
#[cfg(test)]
pub(in crate::util) use render::{SELECTOR_HELP, SELECTOR_INPUT, WORKLOAD_SELECTOR};

/// Which pods a label-following Logs panel streams - the identity its
/// `NavTarget::LabelLogs` dedups on.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(from = "LabelLogsRepr")]
pub enum LabelLogs {
    /// A selector typed into the panel, over the panel's namespaces. Each
    /// such panel is its own instance (`selector-logs-multiple-panels`), so
    /// any number can be open, in one context or several.
    Typed(TypedId),
    /// One workload's pods (a Deployment's, say), by its selector.
    Workload(WorkloadRef),
}

impl LabelLogs {
    /// A new typed panel's source, with an identity of its own.
    pub fn new_typed() -> Self {
        Self::Typed(TypedId::new())
    }

    /// Whether this is a typed panel's source, whichever instance.
    pub fn is_typed(&self) -> bool {
        matches!(self, Self::Typed(_))
    }
}

/// One typed label panel's identity (D1): part of its target, and so of the
/// key the window files it under, which is what lets several be open at once.
/// A new panel's is random and never 0; 0 is what a layout saved before
/// typed panels had identities reads as (D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TypedId(u64);

impl TypedId {
    /// The identity a layout saved before this one existed restores with -
    /// fixed, so the window's key and the restored panel read the same one.
    pub const LEGACY: Self = Self(0);

    /// A fresh identity. Random, so it needs no counter and can't collide
    /// with a restored one in practice; if the OS can't supply randomness,
    /// the clock and a process counter stand in.
    pub fn new() -> Self {
        let id = getrandom::u64().unwrap_or_else(|_| {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(1);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos() as u64);
            nanos ^ COUNTER.fetch_add(1, Ordering::Relaxed).rotate_left(32)
        });
        Self(id.max(1))
    }
}

impl Default for TypedId {
    fn default() -> Self {
        Self::new()
    }
}

/// What `label_logs` deserializes from: today's `{"Typed": <id>}` or
/// `{"Workload": ..}`, or the bare `"Typed"` a layout saved before typed
/// panels had identities holds (D2).
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum LabelLogsRepr {
    Current(CurrentRepr),
    Legacy(LegacyRepr),
}

#[derive(serde::Deserialize)]
enum CurrentRepr {
    Typed(TypedId),
    Workload(WorkloadRef),
}

#[derive(serde::Deserialize)]
enum LegacyRepr {
    Typed,
}

impl From<LabelLogsRepr> for LabelLogs {
    fn from(repr: LabelLogsRepr) -> Self {
        match repr {
            LabelLogsRepr::Current(CurrentRepr::Typed(id)) => Self::Typed(id),
            LabelLogsRepr::Current(CurrentRepr::Workload(workload)) => Self::Workload(workload),
            LabelLogsRepr::Legacy(LegacyRepr::Typed) => Self::Typed(TypedId::LEGACY),
        }
    }
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
            LabelLogs::Typed(_) => "Logs: Labels".to_string(),
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

/// A container to stream, and the source its lines carry: tagged with the
/// pod, plus the container when the pod has several, in the pod's colour.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Wanted {
    pub(super) key: StreamKey,
    pub(super) source: LineSource,
    /// Whether its log grows still, or - terminated - is read to its end.
    pub(super) follow: bool,
}

/// A container streaming into the panel: the task reading it, which stops
/// when dropped, and - for tests - the request it made.
pub(super) struct Streaming {
    #[cfg(test)]
    pub(super) target: LogTarget,
    pub(super) _task: Task<()>,
}

/// How much the selector matched: pods, and the containers in them that have
/// logs to stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Matched {
    pub(super) pods: usize,
    pub(super) containers: usize,
}

/// The pods `selector` picks among `pods` in `namespaces` (all, when empty).
fn matching<'a>(
    pods: &'a [Pod],
    selector: &'a Selector,
    namespaces: &'a [String],
) -> impl Iterator<Item = &'a Pod> {
    pods.iter().filter(move |pod| {
        crate::k8s::resource::pods::matches_namespaces(pod, namespaces)
            && label_selector::matches(selector, pod.metadata.labels.as_ref())
    })
}

fn statuses(pod: &Pod) -> impl Iterator<Item = &ContainerStatus> {
    pod.status
        .as_ref()
        .and_then(|status| status.container_statuses.as_ref())
        .into_iter()
        .flatten()
}

/// The containers `selector` picks among `pods` in `namespaces` (all, when
/// empty) - only those named `container`, when the filter names one - in a
/// stable order, with how many pods and containers that is. A container is
/// streamable once it has started - running, or run and ended - since a
/// waiting one has no log to read yet; it's picked up on the pod's next
/// update.
pub(super) fn wanted(
    pods: &[Pod],
    selector: &Selector,
    namespaces: &[String],
    container: Option<&str>,
) -> (Vec<Wanted>, Matched) {
    let mut matched = Matched::default();
    let mut wanted = Vec::new();
    for pod in matching(pods, selector, namespaces) {
        matched.pods += 1;
        let started: Vec<&ContainerStatus> = statuses(pod)
            .filter(|status| has_started(status))
            .filter(|status| container.is_none_or(|name| status.name == name))
            .collect();
        let several = started.len() > 1;
        let namespace = pod.metadata.namespace.clone().unwrap_or_default();
        let name = pod.metadata.name.clone().unwrap_or_default();
        let uid = pod.metadata.uid.clone().unwrap_or_default();
        for container in started {
            matched.containers += 1;
            let tag = if several {
                format!("{name}/{}", container.name)
            } else {
                name.clone()
            };
            let source = LineSource::new(tag, name.as_str());
            let follow = super::stream::follows(Some(pod), &container.name);
            wanted.push(Wanted {
                key: StreamKey {
                    namespace: namespace.clone(),
                    pod: name.clone(),
                    uid: uid.clone(),
                    container: container.name.clone(),
                    restart_count: container.restart_count,
                },
                source,
                follow,
            });
        }
    }
    wanted.sort_by(|a, b| a.key.cmp(&b.key));
    (wanted, matched)
}

/// Every container name among the pods `selector` picks, sorted: what the
/// container filter offers.
pub(super) fn container_names(
    pods: &[Pod],
    selector: &Selector,
    namespaces: &[String],
) -> Vec<String> {
    let names: std::collections::BTreeSet<&str> = matching(pods, selector, namespaces)
        .flat_map(statuses)
        .map(|status| status.name.as_str())
        .collect();
    names.into_iter().map(str::to_string).collect()
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
    /// The one container name streamed from each pod; `None` streams all.
    pub(super) container: Option<String>,
    /// Every container name among the matched pods, for the filter to offer.
    pub(super) containers: Vec<String>,
    /// A typed panel's namespace picker.
    pub(super) scope: scope::ScopePicker,
    /// A typed panel's selector field, made on its first render.
    pub(super) input: Option<Entity<gpui_kit::component::input::InputState>>,
    /// The context's shared Pods watch, once the panel has subscribed to it.
    pub(super) pods: Option<Entity<crate::k8s::resource::pods::PodsTable>>,
    pub(super) streams: HashMap<StreamKey, Streaming>,
    /// When each stream this view has shown, and since dropped, stopped - so
    /// one that comes back resumes from there instead of replaying lines the
    /// view already holds. Cleared with the view, on a new selector.
    pub(super) stopped: HashMap<StreamKey, jiff::Timestamp>,
    pub(super) matched: Matched,
}

impl Following {
    /// `source`'s state, streaming `selector` - a workload's own, or a typed
    /// panel's restored one - when there is one that parses, filtered to
    /// `container` when that names one.
    pub(super) fn new(
        source: LabelLogs,
        selector: Option<&str>,
        container: Option<String>,
    ) -> Self {
        let selector = match (&source, selector) {
            (LabelLogs::Workload(workload), _) => Some(workload.selector.as_str()),
            (LabelLogs::Typed(_), selector) => selector,
        };
        let selector = selector
            .and_then(|text| label_selector::parse(text).ok())
            .filter(|selector| !selector.selects_all())
            .map(|selector| (selector.to_string(), selector));
        Self {
            source,
            selector,
            error: None,
            container,
            containers: Vec::new(),
            scope: Default::default(),
            input: None,
            pods: None,
            streams: HashMap::new(),
            stopped: HashMap::new(),
            matched: Matched::default(),
        }
    }
}

/// What a saved label-following Logs panel followed.
#[derive(Debug, PartialEq)]
pub(crate) struct SavedLabels {
    pub(crate) source: LabelLogs,
    pub(crate) selector: Option<String>,
    pub(crate) container: Option<String>,
}

/// A saved Logs panel's label source, the selector it streamed and its
/// container filter, if it was a label-following one.
pub(crate) fn labels_from_state(state: &serde_json::Value) -> Option<SavedLabels> {
    let source = serde_json::from_value(state.get("label_logs")?.clone()).ok()?;
    let text = |key: &str| state[key].as_str().map(str::to_string);
    Some(SavedLabels {
        source,
        selector: text("selector"),
        container: text("container_filter"),
    })
}

#[cfg(test)]
mod tests;
