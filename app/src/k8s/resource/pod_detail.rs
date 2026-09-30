//! One Pod's detail panel: a structured field list, or its raw YAML.
//!
//! A dock panel of its own rather than a block inside `PodsPanel`, so detail can
//! be viewed, moved, and closed like any other panel - and so two pods' details
//! can be open side by side. It reads the single `Pod` it was opened for with
//! `Api::get` rather than joining the all-namespaces watch: there is one object,
//! nothing to keep up to date beyond a re-fetch, and no reason to stream a whole
//! cluster's pods into a panel that shows one.

use crate::command::{Command, CommandRegistry};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::resource::pods::format_age;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::{self, PanelScope};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::dock::{
    BasePanel, Panel, PanelControl, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Event as K8sEvent;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::ListParams;

actions!(
    pod_detail,
    [
        ToggleDetailView,
        SelectOverviewTab,
        SelectContainersTab,
        SelectVolumesTab,
        SelectEventsTab,
        SelectManagedFieldsTab
    ]
);

/// This panel's own key context - distinct from `PodsPanel`'s, so the two
/// panels can each bind `y` to a different meaning without conflict (there
/// "open this pod's detail on YAML," here "toggle this already-open panel's
/// view").
pub const PANEL_KEY_CONTEXT: &str = "PodDetailPanel";
const TOGGLE_VIEW_KEY: &str = "y";
const OVERVIEW_TAB_KEY: &str = "1";
const CONTAINERS_TAB_KEY: &str = "2";
const VOLUMES_TAB_KEY: &str = "3";
const EVENTS_TAB_KEY: &str = "4";
const MANAGED_FIELDS_TAB_KEY: &str = "5";

const TOGGLE_VIEW_COMMAND_ID: &str = "pod_detail.toggle_view";
const OVERVIEW_TAB_COMMAND_ID: &str = "pod_detail.tab_overview";
const CONTAINERS_TAB_COMMAND_ID: &str = "pod_detail.tab_containers";
const VOLUMES_TAB_COMMAND_ID: &str = "pod_detail.tab_volumes";
const EVENTS_TAB_COMMAND_ID: &str = "pod_detail.tab_events";
const MANAGED_FIELDS_TAB_COMMAND_ID: &str = "pod_detail.tab_managed_fields";

/// The panel's shortcuts as registry commands, gated to its key context: each
/// gets a palette entry while a detail panel has focus, and a `keymap.toml`
/// override by id. None belongs in the menu bar - they act on one panel, not
/// the app.
pub fn register_commands(registry: &mut CommandRegistry) {
    let commands: [(&'static str, &'static str, &'static str, Box<dyn Action>); 6] = [
        (
            TOGGLE_VIEW_COMMAND_ID,
            "Pod Detail: Toggle Fields/YAML",
            TOGGLE_VIEW_KEY,
            Box::new(ToggleDetailView),
        ),
        (
            OVERVIEW_TAB_COMMAND_ID,
            "Pod Detail: Overview Tab",
            OVERVIEW_TAB_KEY,
            Box::new(SelectOverviewTab),
        ),
        (
            CONTAINERS_TAB_COMMAND_ID,
            "Pod Detail: Containers Tab",
            CONTAINERS_TAB_KEY,
            Box::new(SelectContainersTab),
        ),
        (
            VOLUMES_TAB_COMMAND_ID,
            "Pod Detail: Volumes Tab",
            VOLUMES_TAB_KEY,
            Box::new(SelectVolumesTab),
        ),
        (
            EVENTS_TAB_COMMAND_ID,
            "Pod Detail: Events Tab",
            EVENTS_TAB_KEY,
            Box::new(SelectEventsTab),
        ),
        (
            MANAGED_FIELDS_TAB_COMMAND_ID,
            "Pod Detail: Managed Fields Tab",
            MANAGED_FIELDS_TAB_KEY,
            Box::new(SelectManagedFieldsTab),
        ),
    ];
    for (id, title, default_binding, action) in commands {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(PANEL_KEY_CONTEXT),
            action,
            menu: None,
        });
    }
}

/// The panel's own keybindings, each resolved through `keymap` by its command
/// id - so a `keymap.toml` override rebinds it. Registered with the window's
/// keymap the same way `pods::panel_bindings` is - printing a key in a hint
/// bar does not bind it.
pub fn panel_bindings(keymap: &KeymapConfig) -> [KeyBinding; 6] {
    let key = |id, default| keymap::resolve(id, default, keymap);
    let context = Some(PANEL_KEY_CONTEXT);
    [
        KeyBinding::new(
            &key(TOGGLE_VIEW_COMMAND_ID, TOGGLE_VIEW_KEY),
            ToggleDetailView,
            context,
        ),
        KeyBinding::new(
            &key(OVERVIEW_TAB_COMMAND_ID, OVERVIEW_TAB_KEY),
            SelectOverviewTab,
            context,
        ),
        KeyBinding::new(
            &key(CONTAINERS_TAB_COMMAND_ID, CONTAINERS_TAB_KEY),
            SelectContainersTab,
            context,
        ),
        KeyBinding::new(
            &key(VOLUMES_TAB_COMMAND_ID, VOLUMES_TAB_KEY),
            SelectVolumesTab,
            context,
        ),
        KeyBinding::new(
            &key(EVENTS_TAB_COMMAND_ID, EVENTS_TAB_KEY),
            SelectEventsTab,
            context,
        ),
        KeyBinding::new(
            &key(MANAGED_FIELDS_TAB_COMMAND_ID, MANAGED_FIELDS_TAB_KEY),
            SelectManagedFieldsTab,
            context,
        ),
    ]
}

/// Which tab a structured field belongs to, and the tab strip itself. Kept
/// as a plain enum walked by [`DetailSection::ALL`] rather than deriving from
/// the label string at render time, so a field's tab membership is decided
/// once, at projection time, in `pod_fields`.
///
/// `ManagedFields` is rightmost per feedback on the first cut of this panel -
/// it is the section a reader reaches for least often, being about who wrote
/// a field rather than what the pod is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailSection {
    Overview,
    Containers,
    Volumes,
    Events,
    ManagedFields,
}

impl DetailSection {
    pub const ALL: [DetailSection; 5] = [
        Self::Overview,
        Self::Containers,
        Self::Volumes,
        Self::Events,
        Self::ManagedFields,
    ];

    fn label(self) -> &'static str {
        match self {
            DetailSection::Overview => "Overview",
            DetailSection::Containers => "Containers",
            DetailSection::Volumes => "Volumes",
            DetailSection::Events => "Events",
            DetailSection::ManagedFields => "Managed Fields",
        }
    }
}

/// The label of a field row, and a value shaped so the renderer knows how to
/// draw it without re-deriving that from the string.
///
/// The projection is a plain `&Pod -> Vec<PodField>`, so it is testable as the
/// view model the same way `pod_row` is: a fixture in, asserted rows out, no
/// GPUI harness needed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodField {
    pub label: &'static str,
    pub section: DetailSection,
    pub value: PodFieldValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PodFieldValue {
    /// One short value on the row.
    Text(String),
    /// A value naming another object (its namespace, node, owner, service
    /// account). Styled as a link; not clickable yet - see the change's
    /// `design.md` on the deferred navigation pass.
    Link(String),
    /// Key=value pairs, one chip each.
    Chips(Vec<String>),
    /// Conditions, one badge each.
    Badges(Vec<ConditionBadge>),
    /// Rows behind a disclosure, collapsed by default so a long list does not
    /// push the fields the user came for off screen.
    Collapsed(Vec<String>),
    /// Rows always shown - for a field important enough to have its own tab,
    /// where a Show/Hide toggle would just be an extra click to see the thing
    /// the tab exists for.
    List(Vec<String>),
    /// One card per container - the section a pod detail view exists to show
    /// and a single screenshot never had room to demonstrate in full.
    Containers(Vec<ContainerSummary>),
    /// One block per manager, each independently collapsible: the manager's
    /// name and operation, and (when expanded) the raw `fieldsV1` ownership
    /// tree it wrote - the "who owns what" detail a flat "manager
    /// (operation)" line cannot show.
    ManagedFields(Vec<ManagedFieldEntry>),
}

impl PodFieldValue {
    /// The single-line text this value renders as, for the projection tests
    /// that assert on a value rather than on a rendered element. Composite
    /// values join their parts, so an assertion cannot pass on a value the
    /// user never sees.
    #[cfg(test)]
    pub fn text(&self) -> String {
        match self {
            PodFieldValue::Text(text) | PodFieldValue::Link(text) => text.clone(),
            PodFieldValue::Chips(chips) => chips.join(", "),
            PodFieldValue::Badges(badges) => badges
                .iter()
                .map(|badge| format!("{}={}", badge.condition, badge.status))
                .collect::<Vec<_>>()
                .join(", "),
            PodFieldValue::Collapsed(rows) | PodFieldValue::List(rows) => rows.join(", "),
            PodFieldValue::Containers(containers) => containers
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
            PodFieldValue::ManagedFields(entries) => entries
                .iter()
                .map(|entry| format!("{} ({})", entry.manager, entry.operation))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

/// One manager's ownership entry from `metadata.managedFields`: who wrote
/// what, and the raw ownership tree it claimed - pretty-printed once here
/// rather than at render time, so the render side has no JSON to reason
/// about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedFieldEntry {
    pub manager: String,
    pub operation: String,
    pub fields_json: String,
}

/// One event from the cluster naming this pod, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodEvent {
    pub reason: String,
    pub message: String,
    pub count: i32,
    pub age: String,
    pub tone: BadgeTone,
}

/// One container's summary: spec (image, ports, resources) joined with its
/// live status (ready, restart count, current state) by container name - the
/// two live on different parts of the `Pod` object and only line up by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerSummary {
    pub name: String,
    pub image: String,
    /// `None` before the container has any reported status (e.g. still being
    /// scheduled) - rendered distinctly from a known-not-ready container.
    pub ready: Option<bool>,
    pub restart_count: i32,
    /// "Running", "Waiting: ImagePullBackOff", "Terminated: Completed" - one
    /// human string rather than the raw `ContainerState` union, since the
    /// panel only ever displays it, never branches on which variant it is.
    pub state: String,
    pub ports: Vec<String>,
    pub requests: Vec<String>,
    pub limits: Vec<String>,
}

/// A condition's badge color, decided by the condition rather than looked up at
/// render time - so "True is good, anything else is not" is testable without a
/// theme, and the renderer only maps tone to a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    /// The condition holds.
    Good,
    /// The condition does not hold - a warning to notice, not a failure.
    Warning,
    /// The cluster did not say either way.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConditionBadge {
    /// The condition's `type` (`Ready`, `PodScheduled`, ...).
    pub condition: String,
    /// Its raw `status` (`True`, `False`, `Unknown`).
    pub status: String,
    pub tone: BadgeTone,
}

fn condition_tone(status: &str) -> BadgeTone {
    match status {
        "True" => BadgeTone::Good,
        "False" => BadgeTone::Warning,
        _ => BadgeTone::Unknown,
    }
}

fn chip(key: &str, value: &str) -> String {
    format!("{key}={value}")
}

/// The fields the panel shows, in the order it shows them. Rows whose source is
/// absent from the pod are left out rather than rendered blank - a pod with no
/// owner references has no "Controlled By" row, it does not have an empty one.
pub fn pod_fields(pod: &Pod, now: Timestamp) -> Vec<PodField> {
    let mut fields = Vec::new();
    let mut push = |label: &'static str, section: DetailSection, value: PodFieldValue| {
        fields.push(PodField {
            label,
            section,
            value,
        });
    };

    if let Some(created) = &pod.metadata.creation_timestamp {
        let age_secs = now.duration_since(created.0).as_secs_f64() as i64;
        push(
            "Created",
            DetailSection::Overview,
            PodFieldValue::Text(format!("{} ({})", format_age(age_secs), created.0)),
        );
    }
    if let Some(name) = non_empty(&pod.metadata.name) {
        push(
            "Name",
            DetailSection::Overview,
            PodFieldValue::Text(name.to_string()),
        );
    }
    if let Some(namespace) = non_empty(&pod.metadata.namespace) {
        push(
            "Namespace",
            DetailSection::Overview,
            PodFieldValue::Link(namespace.to_string()),
        );
    }
    let containers = summarize_containers(
        pod.spec
            .as_ref()
            .map(|spec| spec.containers.as_slice())
            .unwrap_or_default(),
        pod.status.as_ref(),
    );
    if !containers.is_empty() {
        push(
            "Containers",
            DetailSection::Containers,
            PodFieldValue::Containers(containers),
        );
    }
    let init_containers = summarize_containers(
        pod.spec
            .as_ref()
            .and_then(|spec| spec.init_containers.as_deref())
            .unwrap_or_default(),
        pod.status.as_ref(),
    );
    if !init_containers.is_empty() {
        push(
            "Init Containers",
            DetailSection::Containers,
            PodFieldValue::Containers(init_containers),
        );
    }
    let volumes: Vec<String> = pod
        .spec
        .as_ref()
        .into_iter()
        .flat_map(|spec| spec.volumes.iter().flatten())
        .map(format_volume)
        .collect();
    if !volumes.is_empty() {
        push(
            "Volumes",
            DetailSection::Volumes,
            PodFieldValue::List(volumes),
        );
    }
    if let Some(labels) = non_empty_map(&pod.metadata.labels) {
        push(
            "Labels",
            DetailSection::Overview,
            PodFieldValue::Chips(labels.iter().map(|(key, value)| chip(key, value)).collect()),
        );
    }
    if let Some(annotations) = non_empty_map(&pod.metadata.annotations) {
        push(
            "Annotations",
            DetailSection::Overview,
            PodFieldValue::Chips(
                annotations
                    .iter()
                    .map(|(key, value)| chip(key, value))
                    .collect(),
            ),
        );
    }
    let owners: Vec<String> = pod
        .metadata
        .owner_references
        .iter()
        .flatten()
        .map(|owner| format!("{}/{}", owner.kind, owner.name))
        .collect();
    if !owners.is_empty() {
        push(
            "Controlled By",
            DetailSection::Overview,
            PodFieldValue::Link(owners.join(", ")),
        );
    }
    let managed_fields: Vec<ManagedFieldEntry> = pod
        .metadata
        .managed_fields
        .iter()
        .flatten()
        .map(managed_field_entry)
        .collect();
    if !managed_fields.is_empty() {
        push(
            "Managed Fields",
            DetailSection::ManagedFields,
            PodFieldValue::ManagedFields(managed_fields),
        );
    }
    if let Some(phase) = pod
        .status
        .as_ref()
        .and_then(|status| non_empty(&status.phase))
    {
        push(
            "Status",
            DetailSection::Overview,
            PodFieldValue::Text(phase.to_string()),
        );
    }
    if let Some(node) = pod
        .spec
        .as_ref()
        .and_then(|spec| non_empty(&spec.node_name))
    {
        push(
            "Node",
            DetailSection::Overview,
            PodFieldValue::Link(node.to_string()),
        );
    }
    if let Some(ips) = ips_of(pod, IpSource::Host) {
        push(
            "Host IPs",
            DetailSection::Overview,
            PodFieldValue::Text(ips),
        );
    }
    if let Some(ips) = ips_of(pod, IpSource::Pod) {
        push("Pod IPs", DetailSection::Overview, PodFieldValue::Text(ips));
    }
    if let Some(account) = pod
        .spec
        .as_ref()
        .and_then(|spec| non_empty(&spec.service_account_name))
    {
        push(
            "Service Account",
            DetailSection::Overview,
            PodFieldValue::Link(account.to_string()),
        );
    }
    if let Some(qos) = pod
        .status
        .as_ref()
        .and_then(|status| non_empty(&status.qos_class))
    {
        push(
            "QoS Class",
            DetailSection::Overview,
            PodFieldValue::Text(qos.to_string()),
        );
    }
    if let Some(grace) = pod
        .spec
        .as_ref()
        .and_then(|spec| spec.termination_grace_period_seconds)
    {
        push(
            "Termination Grace Period",
            DetailSection::Overview,
            PodFieldValue::Text(format_age(grace)),
        );
    }
    let tolerations: Vec<String> = pod
        .spec
        .as_ref()
        .into_iter()
        .flat_map(|spec| spec.tolerations.iter().flatten())
        .map(format_toleration)
        .collect();
    if !tolerations.is_empty() {
        push(
            "Tolerations",
            DetailSection::Overview,
            PodFieldValue::Collapsed(tolerations),
        );
    }
    let conditions: Vec<ConditionBadge> = pod
        .status
        .as_ref()
        .into_iter()
        .flat_map(|status| status.conditions.iter().flatten())
        .map(|condition| ConditionBadge {
            condition: condition.type_.clone(),
            status: condition.status.clone(),
            tone: condition_tone(&condition.status),
        })
        .collect();
    if !conditions.is_empty() {
        push(
            "Conditions",
            DetailSection::Overview,
            PodFieldValue::Badges(conditions),
        );
    }

    fields
}

/// One `managedFields` entry, with its `fieldsV1` ownership tree
/// pretty-printed - or a placeholder noting there was none to print, since an
/// entry missing a manager name still names an operation worth showing rather
/// than being dropped.
fn managed_field_entry(
    entry: &k8s_openapi::apimachinery::pkg::apis::meta::v1::ManagedFieldsEntry,
) -> ManagedFieldEntry {
    let manager = non_empty(&entry.manager)
        .map(str::to_string)
        .unwrap_or_else(|| "unknown manager".to_string());
    let operation = non_empty(&entry.operation)
        .map(str::to_string)
        .unwrap_or_else(|| "unknown operation".to_string());
    let fields_json = entry
        .fields_v1
        .as_ref()
        .and_then(|fields| serde_json::to_string_pretty(&fields.0).ok())
        .unwrap_or_else(|| "(no field ownership recorded)".to_string());
    ManagedFieldEntry {
        manager,
        operation,
        fields_json,
    }
}

/// When an event last happened. The legacy `lastTimestamp`/`firstTimestamp`
/// pair is empty on events written through `events.k8s.io/v1` (the scheduler's
/// `Scheduled`, among others), which record `series.lastObservedTime` and
/// `eventTime` instead - reading only the legacy pair would sort those last and
/// age them "unknown".
fn event_time(event: &K8sEvent) -> Option<Timestamp> {
    event
        .last_timestamp
        .as_ref()
        .map(|time| time.0)
        .or_else(|| {
            event
                .series
                .as_ref()
                .and_then(|series| series.last_observed_time.as_ref())
                .map(|time| time.0)
        })
        .or_else(|| event.event_time.as_ref().map(|time| time.0))
        .or_else(|| event.first_timestamp.as_ref().map(|time| time.0))
}

/// One event's age-and-tone summary, newest first. `Warning`-type events read
/// as a warning tone and any other type (chiefly `Normal`) as good; an event
/// with no type at all is `Unknown`, neither good nor a warning.
fn format_events(events: &[K8sEvent], now: Timestamp) -> Vec<PodEvent> {
    let mut events: Vec<(&K8sEvent, Option<Timestamp>)> = events
        .iter()
        .map(|event| (event, event_time(event)))
        .collect();
    // `Option`'s ordering puts `None` first, so reversing it sorts newest
    // first and leaves undated events at the end.
    events.sort_by(|(_, a), (_, b)| b.cmp(a));
    events
        .into_iter()
        .map(|(event, time)| {
            let age = time
                .map(|time| format_age(now.duration_since(time).as_secs()))
                .unwrap_or_else(|| "unknown".to_string());
            let tone = match event.type_.as_deref() {
                Some("Warning") => BadgeTone::Warning,
                Some(_) => BadgeTone::Good,
                None => BadgeTone::Unknown,
            };
            // Like the timestamps, a series-style event counts its repeats on
            // `series.count` rather than the legacy `count`.
            let count = event
                .count
                .or_else(|| event.series.as_ref().and_then(|series| series.count))
                .unwrap_or(1);
            PodEvent {
                reason: non_empty(&event.reason).unwrap_or("Unknown").to_string(),
                message: non_empty(&event.message).unwrap_or_default().to_string(),
                count,
                age,
                tone,
            }
        })
        .collect()
}

/// Joins `spec.containers` (or `spec.init_containers`) with their matching
/// `status.container_statuses` entry by name - the two live on separate parts
/// of the `Pod` object, and a container with no status yet (still scheduling)
/// still gets a row, just without ready/restart/state data.
fn summarize_containers(
    containers: &[k8s_openapi::api::core::v1::Container],
    status: Option<&k8s_openapi::api::core::v1::PodStatus>,
) -> Vec<ContainerSummary> {
    let statuses = status
        .and_then(|status| status.container_statuses.as_ref())
        .map(|statuses| statuses.as_slice())
        .unwrap_or_default();

    containers
        .iter()
        .map(|container| {
            let matching = statuses.iter().find(|s| s.name == container.name);
            let ports = container
                .ports
                .iter()
                .flatten()
                .map(|port| match non_empty(&port.protocol) {
                    Some(protocol) => format!("{}/{protocol}", port.container_port),
                    None => port.container_port.to_string(),
                })
                .collect();
            let (requests, limits) = container
                .resources
                .as_ref()
                .map(|resources| {
                    (
                        format_quantities(&resources.requests),
                        format_quantities(&resources.limits),
                    )
                })
                .unwrap_or_default();
            ContainerSummary {
                name: container.name.clone(),
                image: container.image.clone().unwrap_or_default(),
                ready: matching.map(|status| status.ready),
                restart_count: matching.map(|status| status.restart_count).unwrap_or(0),
                state: matching
                    .and_then(|status| status.state.as_ref())
                    .map(format_container_state)
                    .unwrap_or_else(|| "Waiting".to_string()),
                ports,
                requests,
                limits,
            }
        })
        .collect()
}

fn format_quantities(
    quantities: &Option<
        std::collections::BTreeMap<String, k8s_openapi::apimachinery::pkg::api::resource::Quantity>,
    >,
) -> Vec<String> {
    quantities
        .iter()
        .flatten()
        .map(|(resource, quantity)| format!("{resource}={}", quantity.0))
        .collect()
}

/// The one human string a `ContainerState` union renders as - a reason when
/// the cluster gave one (`ImagePullBackOff`, `Completed`), the bare state
/// name otherwise.
fn format_container_state(state: &k8s_openapi::api::core::v1::ContainerState) -> String {
    if let Some(running) = &state.running {
        let _ = running;
        return "Running".to_string();
    }
    if let Some(waiting) = &state.waiting {
        return match non_empty(&waiting.reason) {
            Some(reason) => format!("Waiting: {reason}"),
            None => "Waiting".to_string(),
        };
    }
    if let Some(terminated) = &state.terminated {
        return match non_empty(&terminated.reason) {
            Some(reason) => format!("Terminated: {reason}"),
            None => "Terminated".to_string(),
        };
    }
    "Unknown".to_string()
}

/// One volume, named and typed - `ConfigMap: my-config`, `EmptyDir`,
/// `PersistentVolumeClaim: pvc-name` - covering the sources a pod actually
/// uses in practice rather than every `VolumeSource` variant the API defines.
fn format_volume(volume: &k8s_openapi::api::core::v1::Volume) -> String {
    let kind = if let Some(config_map) = &volume.config_map {
        format!("ConfigMap: {}", config_map.name.clone())
    } else if let Some(secret) = &volume.secret {
        format!("Secret: {}", secret.secret_name.clone().unwrap_or_default())
    } else if let Some(pvc) = &volume.persistent_volume_claim {
        format!("PersistentVolumeClaim: {}", pvc.claim_name)
    } else if let Some(host_path) = &volume.host_path {
        format!("HostPath: {}", host_path.path)
    } else if volume.empty_dir.is_some() {
        "EmptyDir".to_string()
    } else {
        "Other".to_string()
    };
    format!("{}: {kind}", volume.name)
}

/// One toleration, in kubectl's key/operator/value/effect shape. Absent pieces
/// are left out rather than printed as `None`.
fn format_toleration(toleration: &k8s_openapi::api::core::v1::Toleration) -> String {
    let operator = toleration.operator.as_deref().unwrap_or("Equal");
    let mut parts = Vec::new();
    match (non_empty(&toleration.key), non_empty(&toleration.value)) {
        // `Exists` matches on the key alone, so a value would be noise.
        (Some(key), _) if operator == "Exists" => parts.push(key.to_string()),
        (Some(key), Some(value)) => parts.push(format!("{key}={value}")),
        (Some(key), None) => parts.push(key.to_string()),
        (None, _) => parts.push(format!("<any> ({operator})")),
    }
    if let Some(effect) = non_empty(&toleration.effect) {
        parts.push(effect.to_string());
    }
    if let Some(seconds) = toleration.toleration_seconds {
        parts.push(format!("for {}", format_age(seconds)));
    }
    parts.join(": ")
}

enum IpSource {
    Host,
    Pod,
}

/// The pod's host or pod IPs, joined for display. Falls back to the singular
/// `host_ip`/`pod_ip` field when the plural one is absent, which is how older
/// clusters report a single address.
fn ips_of(pod: &Pod, source: IpSource) -> Option<String> {
    let status = pod.status.as_ref()?;
    let addresses: Vec<String> = match source {
        IpSource::Host => {
            let plural: Vec<String> = status
                .host_ips
                .iter()
                .flatten()
                .map(|ip| ip.ip.clone())
                .collect();
            if plural.is_empty() {
                status.host_ip.iter().cloned().collect()
            } else {
                plural
            }
        }
        IpSource::Pod => {
            let plural: Vec<String> = status
                .pod_ips
                .iter()
                .flatten()
                .map(|ip| ip.ip.clone())
                .collect();
            if plural.is_empty() {
                status.pod_ip.iter().cloned().collect()
            } else {
                plural
            }
        }
    };
    if addresses.is_empty() {
        None
    } else {
        Some(addresses.join(", "))
    }
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| !value.is_empty())
}

fn non_empty_map(
    map: &Option<std::collections::BTreeMap<String, String>>,
) -> Option<&std::collections::BTreeMap<String, String>> {
    map.as_ref().filter(|map| !map.is_empty())
}

/// Which of the panel's two views is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DetailView {
    /// The structured field list, and the default.
    #[default]
    Structured,
    /// The raw manifest.
    Yaml,
}

/// The events naming a pod, or why they could not be listed. Kept apart from
/// the pod's own fetch result: a user allowed to `get` pods but not to `list`
/// events still gets the pod, and the Events tab says why it is empty rather
/// than claiming there were none.
type PodEvents = Result<Vec<K8sEvent>, String>;

/// What the panel knows about the pod it is scoped to.
enum PodDetailState {
    Loading,
    Loaded(Box<Pod>, PodEvents),
    /// The pod is gone. Its own state rather than an error: a detail panel that
    /// outlives its pod is a normal thing to have left open, not a failure.
    NotFound,
    /// `message` is what the panel shows by default - readable prose, not a
    /// client library's `Debug` dump (`1-window-context-bar` bug 2); `detail`
    /// is that same failure's full technical rendering, kept alongside rather
    /// than discarded.
    Failed {
        message: String,
        detail: String,
    },
}

/// One fetch's outcome, so a 404 is told apart from every other error before it
/// reaches the panel's state.
enum PodFetch {
    Found(Box<Pod>, PodEvents),
    NotFound,
}

/// Fetches the pod and, alongside it, the events naming it - a single round
/// trip's worth of state rather than a second fetch lifecycle to manage, since
/// the Events tab has nothing to show until the pod itself has loaded anyway.
/// A 404 on the pod skips the events lookup entirely: there is nothing left to
/// name events by. Any other failure is `(message, detail)` - see
/// `PodDetailState::Failed`'s doc comment.
async fn fetch_pod(
    client: kube::Client,
    namespace: String,
    name: String,
) -> Result<PodFetch, (String, String)> {
    let api: Api<Pod> = Api::namespaced(client.clone(), &namespace);
    let pod = match api.get(&name).await {
        Ok(pod) => pod,
        Err(kube::Error::Api(status)) if status.code == 404 => return Ok(PodFetch::NotFound),
        Err(error) => {
            return Err((
                crate::k8s::error::describe(&error),
                crate::k8s::error::detail(&error),
            ));
        }
    };
    let events_api: Api<K8sEvent> = Api::namespaced(client, &namespace);
    let events = events_api
        .list(&ListParams::default().fields(&events_selector(&pod, &namespace, &name)))
        .await
        .map(|list| list.items)
        .map_err(|error| error.to_string());
    Ok(PodFetch::Found(Box::new(pod), events))
}

/// The field selector for this pod's events. Name and namespace alone are not
/// enough: a Service or ReplicaSet can share the pod's name, and a StatefulSet
/// pod is recreated under the same name - so the selector also pins the kind
/// and, when the pod has one, its UID, keeping a predecessor's events out.
fn events_selector(pod: &Pod, namespace: &str, name: &str) -> String {
    let mut selector = format!(
        "involvedObject.kind=Pod,involvedObject.namespace={namespace},involvedObject.name={name}"
    );
    if let Some(uid) = non_empty(&pod.metadata.uid) {
        selector.push_str(&format!(",involvedObject.uid={uid}"));
    }
    selector
}

pub fn register_restore(cx: &mut gpui_kit::App) {
    register_panel(cx, "PodDetail", |context, _window, cx| {
        let PanelInfo::Panel(state) = context.info() else {
            panic!("PodDetail layout state must be a panel");
        };
        let context_name = state["context_name"]
            .as_str()
            .expect("PodDetail layout state must name its cluster")
            .to_string();
        let namespace = state["pod_namespace"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let name = state["pod_name"].as_str().unwrap_or_default().to_string();
        let target = NavTarget::pod(namespace.clone(), name.clone());
        let scope = PanelScope::new(target, context_name);
        panel_handle(cx.new(|cx| {
            PodDetailPanel::new(
                PodRef { namespace, name },
                scope,
                DetailView::Structured,
                cx,
            )
        }))
    });
}

/// A dock panel showing one Pod's structured fields, or its raw YAML.
pub struct PodDetailPanel {
    /// Which pod this panel is over - also the identity `PanelKey` dedups on,
    /// through the scope's `NavTarget::Pod`.
    pod: PodRef,
    scope: PanelScope,
    connection: Entity<ClusterConnection>,
    state: PodDetailState,
    viewing: DetailView,
    /// Which tab of the structured view is showing. Irrelevant while
    /// `viewing` is `Yaml`, but kept regardless so switching back to
    /// Structured returns to the tab the user left, not always Overview.
    active_tab: DetailSection,
    /// Which disclosures are expanded: `Collapsed`-value fields (Tolerations)
    /// keyed by field label, and Managed Fields entries keyed `mf-<index>`.
    /// Absent means collapsed - the default for a long list the user came for
    /// something else in.
    open_sections: std::collections::HashSet<String>,
    /// Whether a fetch is in flight, so a connection that flaps does not race
    /// two results into `state`.
    fetching: bool,
    focus_handle: FocusHandle,
}

impl PodDetailPanel {
    pub fn new(pod: PodRef, scope: PanelScope, view: DetailView, cx: &mut Context<Self>) -> Self {
        use crate::k8s::cluster::session::ClusterRegistry;

        let connection = ClusterRegistry::connection(cx, &scope.context_name);
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();

        let mut this = Self {
            pod,
            scope,
            connection,
            state: PodDetailState::Loading,
            viewing: view,
            active_tab: DetailSection::Overview,
            open_sections: std::collections::HashSet::new(),
            fetching: false,
            focus_handle: cx.focus_handle(),
        };
        this.sync(cx);
        this
    }

    /// Construction from an explicit connection, so tests can hand in a stub
    /// instead of the real registry - the same seam `ResourcePanel` uses. The
    /// stub stays non-`Connected`, so nothing spawns a real connect task.
    #[cfg(test)]
    fn with_connection(
        pod: PodRef,
        scope: PanelScope,
        view: DetailView,
        connection: Entity<ClusterConnection>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&connection, |this: &mut Self, _, cx| this.sync(cx))
            .detach();
        let mut this = Self {
            pod,
            scope,
            connection,
            state: PodDetailState::Loading,
            viewing: view,
            active_tab: DetailSection::Overview,
            open_sections: std::collections::HashSet::new(),
            fetching: false,
            focus_handle: cx.focus_handle(),
        };
        this.sync(cx);
        this
    }

    /// Fetches the pod once its cluster has a client. A no-op while a fetch is
    /// in flight or the connection is not up yet - the observer on the
    /// connection calls this again once it is.
    fn sync(&mut self, cx: &mut Context<Self>) {
        if self.fetching {
            return;
        }
        let ConnectionState::Connected(client) = &self.connection.read(cx).state else {
            return;
        };
        let client = client.clone();
        let namespace = self.pod.namespace.clone();
        let name = self.pod.name.clone();
        self.fetching = true;
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx.send(fetch_pod(client, namespace, name).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.fetching = false;
                    this.state = match result {
                        Ok(PodFetch::Found(pod, events)) => PodDetailState::Loaded(pod, events),
                        Ok(PodFetch::NotFound) => PodDetailState::NotFound,
                        Err((message, detail)) => PodDetailState::Failed { message, detail },
                    };
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    /// Switches the view. Called by the toolbar toggle, and by `MainWindow`
    /// when `nav.show_pod_detail_yaml` is dispatched for a panel that is
    /// already open - `y` has to land on the YAML even when the panel exists.
    pub fn set_view(&mut self, view: DetailView, cx: &mut Context<Self>) {
        self.viewing = view;
        cx.notify();
    }

    /// Which view is showing. The assertion hook for the window's
    /// switch-on-focus path, which cannot read `viewing` across the module
    /// boundary - hence test-only, like [`Self::with_connection`].
    #[cfg(test)]
    pub fn view(&self) -> DetailView {
        self.viewing
    }

    /// Switches the active tab of the structured view.
    fn set_active_tab(&mut self, section: DetailSection, cx: &mut Context<Self>) {
        self.active_tab = section;
        cx.notify();
    }

    /// The active tab. Test-only, like [`Self::view`].
    #[cfg(test)]
    pub fn active_tab(&self) -> DetailSection {
        self.active_tab
    }

    /// The loaded pod, if the fetch has landed. Read by tests and by render.
    fn pod(&self) -> Option<&Pod> {
        match &self.state {
            PodDetailState::Loaded(pod, _) => Some(pod),
            _ => None,
        }
    }

    /// The events naming this pod, once the fetch has landed - or why they
    /// could not be listed. `None` until then.
    fn events(&self) -> Option<&PodEvents> {
        match &self.state {
            PodDetailState::Loaded(_, events) => Some(events),
            _ => None,
        }
    }

    /// The rows the structured view renders, or empty while there is nothing to
    /// show. Projected here rather than in `render` so a test asserts the rows
    /// the panel would draw.
    fn fields(&self, now: Timestamp) -> Vec<PodField> {
        self.pod()
            .map(|pod| pod_fields(pod, now))
            .unwrap_or_default()
    }

    /// The raw manifest, or nothing while there is no pod to render. Shared by
    /// the YAML view and its test, so the test reads exactly what the view
    /// draws rather than a second serialization that could drift from it.
    fn yaml(&self) -> Option<String> {
        self.pod()
            .and_then(|pod| serde_yaml_ng::to_string(pod).ok())
    }

    fn render_field(&self, field: &PodField, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let value =
            match &field.value {
                PodFieldValue::Text(text) => div().child(text.clone()).into_any_element(),
                // Link-styled but not yet clickable - see `design.md` on the
                // deferred navigation pass.
                PodFieldValue::Link(text) => div()
                    .text_color(theme.primary)
                    .child(text.clone())
                    .into_any_element(),
                PodFieldValue::Chips(chips) => div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(chips.iter().map(|chip| {
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(theme.muted)
                            .text_sm()
                            .child(chip.clone())
                    }))
                    .into_any_element(),
                PodFieldValue::Badges(badges) => div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(badges.iter().map(|badge| {
                        let color = match badge.tone {
                            BadgeTone::Good => theme.success,
                            BadgeTone::Warning => theme.warning,
                            BadgeTone::Unknown => theme.muted_foreground,
                        };
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(color)
                            .text_color(theme.background)
                            .text_sm()
                            .child(badge.condition.clone())
                    }))
                    .into_any_element(),
                PodFieldValue::Collapsed(rows) => {
                    // Bound before the closure: the id is used twice and the
                    // closure is `'static`, so it cannot borrow `field`.
                    let label = field.label;
                    let open = self.open_sections.contains(label);
                    let this = cx.weak_entity();
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Button::new(label)
                                .label(if open { "Hide" } else { "Show" })
                                .xsmall()
                                .ghost()
                                .tab_stop(false)
                                .on_click(move |_event, _window, cx| {
                                    let _ = this.update(cx, |this: &mut Self, cx| {
                                        if !this.open_sections.remove(label) {
                                            this.open_sections.insert(label.to_string());
                                        }
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(Collapsible::new().open(open).content(
                            div().flex().flex_col().children(
                                rows.iter().map(|row| div().text_sm().child(row.clone())),
                            ),
                        ))
                        .into_any_element()
                }
                PodFieldValue::List(rows) => div()
                    .flex()
                    .flex_col()
                    .children(rows.iter().map(|row| div().text_sm().child(row.clone())))
                    .into_any_element(),
                PodFieldValue::Containers(containers) => {
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(containers.iter().map(|container| {
                            let ready_color = match container.ready {
                                Some(true) => theme.success,
                                Some(false) => theme.warning,
                                None => theme.muted_foreground,
                            };
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .p_2()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(div().size(px(8.)).rounded_full().bg(ready_color))
                                        .child(
                                            div()
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(container.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(theme.muted_foreground)
                                                .child(container.state.clone()),
                                        ),
                                )
                                .child(div().text_sm().min_w_0().child(container.image.clone()))
                                .when(container.restart_count > 0, |this| {
                                    this.child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.warning)
                                            .child(format!("{} restarts", container.restart_count)),
                                    )
                                })
                                .when(!container.ports.is_empty(), |this| {
                                    this.child(
                                        div().text_sm().text_color(theme.muted_foreground).child(
                                            format!("Ports: {}", container.ports.join(", ")),
                                        ),
                                    )
                                })
                                .when(!container.requests.is_empty(), |this| {
                                    this.child(
                                        div().text_sm().text_color(theme.muted_foreground).child(
                                            format!("Requests: {}", container.requests.join(", ")),
                                        ),
                                    )
                                })
                                .when(!container.limits.is_empty(), |this| {
                                    this.child(
                                        div().text_sm().text_color(theme.muted_foreground).child(
                                            format!("Limits: {}", container.limits.join(", ")),
                                        ),
                                    )
                                })
                        }))
                        .into_any_element()
                }
                PodFieldValue::ManagedFields(entries) => div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(entries.iter().enumerate().map(|(index, entry)| {
                        // Keyed by index, not manager name: two entries can
                        // share a manager (a status subresource update versus
                        // the main resource), and collapsing them onto one key
                        // would toggle both at once.
                        let key: SharedString = format!("mf-{index}").into();
                        let open = self.open_sections.contains(key.as_ref());
                        let this = cx.weak_entity();
                        let key_for_click = key.clone();
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(format!("{}: {}", entry.manager, entry.operation))
                                    .child(
                                        Button::new(key)
                                            .label(if open { "Hide" } else { "Show" })
                                            .xsmall()
                                            .ghost()
                                            .tab_stop(false)
                                            .on_click(move |_event, _window, cx| {
                                                let _ = this.update(cx, |this: &mut Self, cx| {
                                                    if !this
                                                        .open_sections
                                                        .remove(key_for_click.as_ref())
                                                    {
                                                        this.open_sections
                                                            .insert(key_for_click.to_string());
                                                    }
                                                    cx.notify();
                                                });
                                            }),
                                    ),
                            )
                            .child(
                                // Wraps rather than `whitespace_nowrap()` like the
                                // YAML view: the structured view scrolls vertically
                                // only, so an unwrapped deep ownership path would
                                // push the panel wider than it is.
                                Collapsible::new().open(open).content(
                                    div()
                                        .font_family(theme.mono_font_family.clone())
                                        .text_sm()
                                        .child(entry.fields_json.clone()),
                                ),
                            )
                            .into_any_element()
                    }))
                    .into_any_element(),
            };

        div()
            .flex()
            .gap_3()
            .py_1()
            .child(
                div()
                    .w(px(180.))
                    .flex_none()
                    .text_color(theme.muted_foreground)
                    .child(field.label),
            )
            // `min_w_0()`: without it a flex child sized by its own content
            // (a long annotation value, an unbroken image reference) refuses
            // to shrink below that content's width and pushes the row - and
            // the panel - wider instead of wrapping.
            .child(div().flex_1().min_w_0().child(value))
            .into_any_element()
    }

    fn render_structured(&self, cx: &Context<Self>) -> AnyElement {
        let fields = self.fields(Timestamp::now());
        let active_tab = self.active_tab;
        let this = cx.weak_entity();
        let tabs = TabBar::new("pod-detail-tabs")
            .selected_index(
                DetailSection::ALL
                    .iter()
                    .position(|section| *section == active_tab)
                    .unwrap_or(0),
            )
            .on_click(move |ix, _window, cx| {
                let Some(section) = DetailSection::ALL.get(*ix).copied() else {
                    return;
                };
                let _ = this.update(cx, |this: &mut Self, cx| this.set_active_tab(section, cx));
            })
            .children(
                DetailSection::ALL
                    .iter()
                    .map(|section| Tab::new().label(section.label())),
            );
        let content = if active_tab == DetailSection::Events {
            self.render_events(cx)
        } else {
            div()
                .flex()
                .flex_col()
                .children(
                    fields
                        .iter()
                        .filter(|field| field.section == active_tab)
                        .map(|field| self.render_field(field, cx)),
                )
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .child(tabs)
            .child(div().flex().flex_col().pt_2().child(content))
            .into_any_element()
    }

    /// The Events tab: every event naming this pod, newest first. Its own
    /// render path rather than a `PodField` - events come from a separate
    /// fetch, not from `pod_fields`'s projection of the pod object itself.
    fn render_events(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let events = match self.events() {
            Some(Ok(events)) => format_events(events, Timestamp::now()),
            Some(Err(reason)) => {
                return div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(format!("Could not list events: {reason}"))
                    .into_any_element();
            }
            None => Vec::new(),
        };
        if events.is_empty() {
            return div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("No events.")
                .into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(events.iter().map(|event| {
                let reason_color = match event.tone {
                    BadgeTone::Good => theme.foreground,
                    BadgeTone::Warning => theme.warning,
                    BadgeTone::Unknown => theme.muted_foreground,
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(reason_color)
                                    .child(event.reason.clone()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("{} · x{}", event.age, event.count)),
                            ),
                    )
                    .child(div().text_sm().child(event.message.clone()))
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn render_yaml(&self, cx: &App) -> AnyElement {
        let Some(yaml) = self.yaml() else {
            return div().into_any_element();
        };
        div()
            .size_full()
            .font_family(cx.theme().mono_font_family.clone())
            .whitespace_nowrap()
            .child(yaml)
            .into_any_element()
    }
}

impl Focusable for PodDetailPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for PodDetailPanel {}

impl PodDetailPanel {
    fn on_action_toggle_view(
        &mut self,
        _: &ToggleDetailView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = match self.viewing {
            DetailView::Structured => DetailView::Yaml,
            DetailView::Yaml => DetailView::Structured,
        };
        self.set_view(next, cx);
    }

    fn on_action_select_overview_tab(
        &mut self,
        _: &SelectOverviewTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::Overview, cx);
    }

    fn on_action_select_containers_tab(
        &mut self,
        _: &SelectContainersTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::Containers, cx);
    }

    fn on_action_select_volumes_tab(
        &mut self,
        _: &SelectVolumesTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::Volumes, cx);
    }

    fn on_action_select_events_tab(
        &mut self,
        _: &SelectEventsTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::Events, cx);
    }

    fn on_action_select_managed_fields_tab(
        &mut self,
        _: &SelectManagedFieldsTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_active_tab(DetailSection::ManagedFields, cx);
    }
}

impl Render for PodDetailPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.state {
            PodDetailState::Loading => div()
                .size_full()
                .p_3()
                .child("Loading pod...")
                .into_any_element(),
            PodDetailState::NotFound => div()
                .size_full()
                .p_3()
                .child("This pod no longer exists.")
                .into_any_element(),
            PodDetailState::Failed { message, detail } => panel_title::error_content(
                format!("Could not read pod: {message}"),
                Some(detail.clone()),
                cx,
            )
            .into_any_element(),
            PodDetailState::Loaded(_, _) => match self.viewing {
                // Field values wrap to the panel's width rather than
                // overflowing it - vertical-only scroll, so nothing pushes
                // the layout wider than the panel actually is.
                DetailView::Structured => div()
                    .size_full()
                    .p_3()
                    .overflow_y_scrollbar()
                    .child(self.render_structured(cx))
                    .into_any_element(),
                // YAML is monospace and line-oriented like the Logs panel -
                // it keeps both-axis scroll rather than wrapping lines.
                DetailView::Yaml => div()
                    .size_full()
                    .p_3()
                    .overflow_scrollbar()
                    .child(self.render_yaml(cx))
                    .into_any_element(),
            },
        };

        // The structured/YAML toggle, with the same visible-shortcut-hint
        // convention `PodsPanel` uses - inside the panel's own body, not the
        // dock's shared per-tab-group toolbar, which only reflects whichever
        // tab happens to be active.
        let window_contexts = crate::util::shell::window_context_count(window, cx);
        let yaml = self.viewing == DetailView::Yaml;
        let toggle_key =
            Kbd::binding_for_action(&ToggleDetailView, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| {
                    Kbd::new(Keystroke::parse(TOGGLE_VIEW_KEY).expect("valid keybinding"))
                });
        let toggle_hint = div()
            .flex()
            .items_center()
            .gap_1()
            .child(toggle_key)
            .child(if yaml { "Show fields" } else { "Show YAML" });
        let tab_key = |section: DetailSection| -> Kbd {
            let (action, literal): (&dyn Action, &str) = match section {
                DetailSection::Overview => (&SelectOverviewTab as &dyn Action, OVERVIEW_TAB_KEY),
                DetailSection::Containers => {
                    (&SelectContainersTab as &dyn Action, CONTAINERS_TAB_KEY)
                }
                DetailSection::Volumes => (&SelectVolumesTab as &dyn Action, VOLUMES_TAB_KEY),
                DetailSection::Events => (&SelectEventsTab as &dyn Action, EVENTS_TAB_KEY),
                DetailSection::ManagedFields => (
                    &SelectManagedFieldsTab as &dyn Action,
                    MANAGED_FIELDS_TAB_KEY,
                ),
            };
            Kbd::binding_for_action(action, Some(PANEL_KEY_CONTEXT), window)
                .unwrap_or_else(|| Kbd::new(Keystroke::parse(literal).expect("valid keybinding")))
        };
        // The pod's name (and, in a multi-context window, its context) on the left;
        // the tab and view-toggle hints on the right.
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .p_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().min_w_0().child(panel_title::item_heading(
                self.pod.name.clone(),
                panel_title::heading_context(&self.scope, window_contexts),
                cx.theme().muted_foreground,
            )))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .when(!yaml, |this| {
                        this.children(DetailSection::ALL.iter().map(|section| {
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(tab_key(*section))
                                .child(section.label())
                        }))
                    })
                    .child(toggle_hint),
            );

        let body = div()
            .size_full()
            .key_context(PANEL_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_toggle_view))
            .on_action(cx.listener(Self::on_action_select_overview_tab))
            .on_action(cx.listener(Self::on_action_select_containers_tab))
            .on_action(cx.listener(Self::on_action_select_volumes_tab))
            .on_action(cx.listener(Self::on_action_select_events_tab))
            .on_action(cx.listener(Self::on_action_select_managed_fields_tab))
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().child(content));

        panel_title::focus_frame(body, &self.focus_handle, window, cx)
    }
}

impl BasePanel for PodDetailPanel {
    fn panel_name(&self) -> &'static str {
        "PodDetail"
    }

    fn dump(&self, _cx: &App) -> PanelState {
        PanelState {
            panel_name: self.panel_name().to_string(),
            children: Vec::new(),
            info: PanelInfo::Panel(serde_json::json!({
                "context_name": self.scope.context_name,
                "pod_namespace": self.pod.namespace,
                "pod_name": self.pod.name,
            })),
        }
    }
}

/// The title bar, supplied to the dock rather than drawn here - the same bar
/// every other panel gets, so `"Pod: <name>"` comes from section 2's rule
/// rather than a second implementation in this panel.
///
/// No namespace picker: the panel is over one pod, not a namespace-scoped list,
/// so there is no scope for a picker to change.
impl Panel for PodDetailPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        panel_title::title_element(&self.scope, panel_title::title(&self.scope))
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        panel_title::tab_name(&self.scope)
    }

    fn toolbar_buttons(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Vec<Button>> {
        panel_title::toolbar_buttons()
    }

    fn zoom_control(&self, _cx: &App) -> Option<PanelControl> {
        Some(PanelControl::Toolbar)
    }
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: `gpui_kit::*` re-exports its own `test` macro, which
    // would shadow `core::prelude::v1::test` for these plain synchronous tests.
    use super::{
        BadgeTone, DetailSection, DetailView, K8sEvent, ManagedFieldEntry, PodDetailPanel,
        PodDetailState, PodEvent, PodFetch, PodField, PodFieldValue, events_selector, fetch_pod,
        format_age, format_events, managed_field_entry, pod_fields,
    };
    use crate::command::CommandRegistry;
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::keymap::KeymapConfig;
    use crate::ui::nav::{NavTarget, PodRef};
    use crate::ui::panel_title::PanelScope;
    use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
    use jiff::Timestamp;
    use k8s_openapi::api::core::v1::{
        Container, EventSeries, HostIP, Pod, PodCondition, PodIP, PodSpec, PodStatus, Toleration,
    };
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
        FieldsV1, ManagedFieldsEntry, MicroTime, ObjectMeta, OwnerReference, Time,
    };

    fn field<'a>(fields: &'a [PodField], label: &str) -> Option<&'a PodField> {
        fields.iter().find(|field| field.label == label)
    }

    /// A pod carrying every field the projection reads, so a test can assert
    /// the whole list rather than each field in isolation.
    fn rich_pod() -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some("api-7d9f-ftg5t".into()),
                namespace: Some("staging".into()),
                creation_timestamp: Some(Time(Timestamp::from_second(0).unwrap())),
                labels: Some(
                    [
                        ("app".to_string(), "api".to_string()),
                        ("tier".to_string(), "backend".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                annotations: Some(
                    [
                        ("team".to_string(), "platform".to_string()),
                        ("checked".to_string(), "yes".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                owner_references: Some(vec![OwnerReference {
                    api_version: "apps/v1".into(),
                    kind: "ReplicaSet".into(),
                    name: "api-7d9f".into(),
                    uid: "owner-1".into(),
                    controller: Some(true),
                    ..Default::default()
                }]),
                managed_fields: Some(vec![
                    ManagedFieldsEntry {
                        manager: Some("kubelet".into()),
                        operation: Some("Update".into()),
                        ..Default::default()
                    },
                    ManagedFieldsEntry {
                        manager: Some("kube-controller-manager".into()),
                        operation: Some("Update".into()),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            },
            spec: Some(PodSpec {
                node_name: Some("node-a".into()),
                service_account_name: Some("api".into()),
                termination_grace_period_seconds: Some(30),
                tolerations: Some(vec![Toleration {
                    key: Some("dedicated".into()),
                    operator: Some("Equal".into()),
                    value: Some("api".into()),
                    effect: Some("NoSchedule".into()),
                    ..Default::default()
                }]),
                containers: vec![Container {
                    name: "app".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some("Running".into()),
                qos_class: Some("Burstable".into()),
                host_ips: Some(vec![HostIP {
                    ip: "10.0.0.1".into(),
                }]),
                pod_ips: Some(vec![PodIP {
                    ip: "10.1.0.7".into(),
                }]),
                conditions: Some(vec![
                    PodCondition {
                        type_: "Ready".into(),
                        status: "True".into(),
                        ..Default::default()
                    },
                    PodCondition {
                        type_: "PodScheduled".into(),
                        status: "False".into(),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            }),
        }
    }

    /// Section 3.1: every field the design lists is present, in order, and a
    /// pod with more than one label/annotation/toleration/condition keeps them
    /// all as discrete items rather than collapsing them into one string.
    #[test]
    fn every_structured_field_is_projected_in_order() {
        let fields = pod_fields(&rich_pod(), Timestamp::from_second(90).unwrap());

        let labels: Vec<&str> = fields.iter().map(|f| f.label).collect();
        assert_eq!(
            labels,
            vec![
                "Created",
                "Name",
                "Namespace",
                "Containers",
                "Labels",
                "Annotations",
                "Controlled By",
                "Managed Fields",
                "Status",
                "Node",
                "Host IPs",
                "Pod IPs",
                "Service Account",
                "QoS Class",
                "Termination Grace Period",
                "Tolerations",
                "Conditions",
            ]
        );

        assert_eq!(
            field(&fields, "Created").unwrap().value.text(),
            "1m (1970-01-01T00:00:00Z)"
        );
        assert_eq!(
            field(&fields, "Name").unwrap().value.text(),
            "api-7d9f-ftg5t"
        );
        assert_eq!(field(&fields, "Namespace").unwrap().value.text(), "staging");
        assert_eq!(
            field(&fields, "Labels").unwrap().value,
            PodFieldValue::Chips(vec!["app=api".into(), "tier=backend".into()]),
            "one chip per label"
        );
        assert_eq!(
            field(&fields, "Annotations").unwrap().value,
            PodFieldValue::Chips(vec!["checked=yes".into(), "team=platform".into()])
        );
        assert_eq!(
            field(&fields, "Controlled By").unwrap().value,
            PodFieldValue::Link("ReplicaSet/api-7d9f".into())
        );
        assert_eq!(field(&fields, "Status").unwrap().value.text(), "Running");
        let PodFieldValue::ManagedFields(managed) =
            &field(&fields, "Managed Fields").unwrap().value
        else {
            panic!("managed fields render as PodFieldValue::ManagedFields");
        };
        assert_eq!(
            managed
                .iter()
                .map(|entry| format!("{} ({})", entry.manager, entry.operation))
                .collect::<Vec<_>>(),
            vec![
                "kubelet (Update)".to_string(),
                "kube-controller-manager (Update)".to_string(),
            ],
            "one entry per manager"
        );
        assert_eq!(field(&fields, "Node").unwrap().value.text(), "node-a");
        assert_eq!(field(&fields, "Host IPs").unwrap().value.text(), "10.0.0.1");
        assert_eq!(field(&fields, "Pod IPs").unwrap().value.text(), "10.1.0.7");
        assert_eq!(
            field(&fields, "Service Account").unwrap().value.text(),
            "api"
        );
        assert_eq!(
            field(&fields, "QoS Class").unwrap().value.text(),
            "Burstable"
        );
        assert_eq!(
            field(&fields, "Termination Grace Period")
                .unwrap()
                .value
                .text(),
            "30s"
        );
        assert_eq!(
            field(&fields, "Tolerations").unwrap().value,
            PodFieldValue::Collapsed(vec!["dedicated=api: NoSchedule".into()])
        );
    }

    /// Section 1.1: every field's section matches design.md's grouping table,
    /// and every field the projection produces lands in exactly one tab.
    #[test]
    fn every_field_is_grouped_into_its_designed_section() {
        let fields = pod_fields(&rich_pod(), Timestamp::from_second(90).unwrap());

        let expected: &[(&str, DetailSection)] = &[
            ("Created", DetailSection::Overview),
            ("Name", DetailSection::Overview),
            ("Namespace", DetailSection::Overview),
            ("Containers", DetailSection::Containers),
            ("Labels", DetailSection::Overview),
            ("Annotations", DetailSection::Overview),
            ("Controlled By", DetailSection::Overview),
            ("Managed Fields", DetailSection::ManagedFields),
            ("Status", DetailSection::Overview),
            ("Node", DetailSection::Overview),
            ("Host IPs", DetailSection::Overview),
            ("Pod IPs", DetailSection::Overview),
            ("Service Account", DetailSection::Overview),
            ("QoS Class", DetailSection::Overview),
            ("Termination Grace Period", DetailSection::Overview),
            ("Tolerations", DetailSection::Overview),
            ("Conditions", DetailSection::Overview),
        ];
        for (label, section) in expected {
            assert_eq!(
                field(&fields, label).unwrap().section,
                *section,
                "{label} is in the wrong tab"
            );
        }
        assert_eq!(
            fields.len(),
            expected.len(),
            "every projected field is accounted for above"
        );
    }

    /// Section 3.1: a condition's badge tone follows its own status, so the
    /// renderer cannot color a `False` condition green.
    #[test]
    fn condition_badges_tone_by_their_status() {
        let fields = pod_fields(&rich_pod(), Timestamp::from_second(0).unwrap());
        let PodFieldValue::Badges(badges) = &field(&fields, "Conditions").unwrap().value else {
            panic!("conditions render as badges");
        };

        assert_eq!(badges.len(), 2, "one badge per condition");
        assert_eq!(badges[0].condition, "Ready");
        assert_eq!(badges[0].status, "True");
        assert_eq!(badges[0].tone, BadgeTone::Good);
        assert_eq!(badges[1].condition, "PodScheduled");
        assert_eq!(badges[1].status, "False");
        assert_eq!(
            badges[1].tone,
            BadgeTone::Warning,
            "a condition that does not hold is not green"
        );
    }

    /// Section: containers and volumes are the section a pod detail view
    /// exists to show - image, ready state, restart count, ports, and
    /// resource requests/limits, joined from `spec.containers` and its
    /// matching `status.container_statuses` entry by name.
    #[test]
    fn containers_join_spec_and_status_by_name() {
        use k8s_openapi::api::core::v1::{
            ContainerPort, ContainerState, ContainerStateRunning, ContainerStatus,
            ResourceRequirements,
        };
        use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

        let mut pod = rich_pod();
        pod.spec.as_mut().unwrap().containers = vec![Container {
            name: "app".into(),
            image: Some("registry.example/app:1.2.3".into()),
            ports: Some(vec![ContainerPort {
                container_port: 8080,
                protocol: Some("TCP".into()),
                ..Default::default()
            }]),
            resources: Some(ResourceRequirements {
                requests: Some(
                    [("cpu".to_string(), Quantity("100m".into()))]
                        .into_iter()
                        .collect(),
                ),
                limits: Some(
                    [("memory".to_string(), Quantity("256Mi".into()))]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        }];
        pod.status.as_mut().unwrap().container_statuses = Some(vec![ContainerStatus {
            name: "app".into(),
            ready: true,
            restart_count: 3,
            state: Some(ContainerState {
                running: Some(ContainerStateRunning::default()),
                ..Default::default()
            }),
            ..Default::default()
        }]);

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        let PodFieldValue::Containers(containers) = &field(&fields, "Containers").unwrap().value
        else {
            panic!("containers render as PodFieldValue::Containers");
        };

        assert_eq!(containers.len(), 1);
        let app = &containers[0];
        assert_eq!(app.name, "app");
        assert_eq!(app.image, "registry.example/app:1.2.3");
        assert_eq!(app.ready, Some(true));
        assert_eq!(app.restart_count, 3);
        assert_eq!(app.state, "Running");
        assert_eq!(app.ports, vec!["8080/TCP"]);
        assert_eq!(app.requests, vec!["cpu=100m"]);
        assert_eq!(app.limits, vec!["memory=256Mi"]);
    }

    /// A container with no status yet (still scheduling) still gets a row -
    /// it just does not know ready/restart/state.
    #[test]
    fn a_container_with_no_status_yet_still_gets_a_row() {
        let mut pod = rich_pod();
        pod.spec.as_mut().unwrap().containers = vec![Container {
            name: "app".into(),
            ..Default::default()
        }];
        pod.status.as_mut().unwrap().container_statuses = None;

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        let PodFieldValue::Containers(containers) = &field(&fields, "Containers").unwrap().value
        else {
            panic!("containers render as PodFieldValue::Containers");
        };

        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].ready, None);
        assert_eq!(containers[0].restart_count, 0);
        assert_eq!(containers[0].state, "Waiting");
    }

    /// Volumes name and type each source a pod actually uses.
    #[test]
    fn volumes_are_named_and_typed() {
        use k8s_openapi::api::core::v1::{
            ConfigMapVolumeSource, EmptyDirVolumeSource, PersistentVolumeClaimVolumeSource, Volume,
        };

        let mut pod = rich_pod();
        pod.spec.as_mut().unwrap().volumes = Some(vec![
            Volume {
                name: "config".into(),
                config_map: Some(ConfigMapVolumeSource {
                    name: "app-config".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            Volume {
                name: "data".into(),
                persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                    claim_name: "app-data".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            Volume {
                name: "scratch".into(),
                empty_dir: Some(EmptyDirVolumeSource::default()),
                ..Default::default()
            },
        ]);

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        let PodFieldValue::List(volumes) = &field(&fields, "Volumes").unwrap().value else {
            panic!("volumes render as PodFieldValue::List");
        };

        assert_eq!(
            volumes,
            &vec![
                "config: ConfigMap: app-config".to_string(),
                "data: PersistentVolumeClaim: app-data".to_string(),
                "scratch: EmptyDir".to_string(),
            ]
        );
    }

    /// A condition the cluster did not resolve reads as neither good nor bad.
    #[test]
    fn an_unresolved_condition_is_neither_good_nor_a_warning() {
        let mut pod = rich_pod();
        pod.status.as_mut().unwrap().conditions = Some(vec![PodCondition {
            type_: "Ready".into(),
            status: "Unknown".into(),
            ..Default::default()
        }]);

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        let PodFieldValue::Badges(badges) = &field(&fields, "Conditions").unwrap().value else {
            panic!("conditions render as badges");
        };
        assert_eq!(badges[0].tone, BadgeTone::Unknown);
    }

    /// Section 3.2: rows whose source is absent are left out, not shown blank -
    /// so a minimal pod reads as a short list, not a page of empty values.
    #[test]
    fn absent_fields_are_omitted_rather_than_blank() {
        let minimal = Pod {
            metadata: ObjectMeta {
                name: Some("bare".into()),
                namespace: Some("default".into()),
                ..Default::default()
            },
            ..Default::default()
        };

        let fields = pod_fields(&minimal, Timestamp::from_second(0).unwrap());
        for absent in [
            "Created",
            "Labels",
            "Annotations",
            "Controlled By",
            "Managed Fields",
            "Status",
            "Node",
            "Host IPs",
            "Pod IPs",
            "Service Account",
            "QoS Class",
            "Termination Grace Period",
            "Tolerations",
            "Conditions",
        ] {
            assert!(
                field(&fields, absent).is_none(),
                "{absent} has no source on a pod without one"
            );
        }
        assert_eq!(
            fields.iter().map(|f| f.label).collect::<Vec<_>>(),
            vec!["Name", "Namespace"],
            "only the fields the pod actually has"
        );
    }

    /// Section 3.2's "a single IP" case: a cluster reporting one address in the
    /// singular field still gets a row, rather than no row at all.
    #[test]
    fn a_single_ip_reported_singularly_still_renders() {
        let mut pod = rich_pod();
        let status = pod.status.as_mut().unwrap();
        status.host_ips = None;
        status.host_ip = Some("192.168.1.5".into());
        status.pod_ips = None;
        status.pod_ip = Some("192.168.1.6".into());

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        assert_eq!(
            field(&fields, "Host IPs").unwrap().value.text(),
            "192.168.1.5"
        );
        assert_eq!(
            field(&fields, "Pod IPs").unwrap().value.text(),
            "192.168.1.6"
        );
    }

    /// An `Exists` toleration matches on its key alone, so a value it does not
    /// have must not be invented for it.
    #[test]
    fn an_exists_toleration_reads_by_its_key_alone() {
        let mut pod = rich_pod();
        pod.spec.as_mut().unwrap().tolerations = Some(vec![Toleration {
            key: Some("node-role.kubernetes.io/control-plane".into()),
            operator: Some("Exists".into()),
            effect: Some("NoSchedule".into()),
            ..Default::default()
        }]);

        let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
        assert_eq!(
            field(&fields, "Tolerations").unwrap().value,
            PodFieldValue::Collapsed(vec![
                "node-role.kubernetes.io/control-plane: NoSchedule".into()
            ])
        );
    }

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).unwrap()
    }

    #[test]
    fn events_sort_newest_first_across_legacy_and_series_timestamps() {
        let legacy = K8sEvent {
            reason: Some("Pulled".into()),
            message: Some("Container image already present".into()),
            type_: Some("Normal".into()),
            count: Some(2),
            first_timestamp: Some(Time(at(50))),
            last_timestamp: Some(Time(at(100))),
            ..Default::default()
        };
        // Written through events.k8s.io/v1: no legacy timestamps or count, only
        // `eventTime` and a series.
        let series = K8sEvent {
            reason: Some("BackOff".into()),
            message: Some("Back-off restarting failed container".into()),
            type_: Some("Warning".into()),
            event_time: Some(MicroTime(at(200))),
            series: Some(EventSeries {
                count: Some(4),
                last_observed_time: Some(MicroTime(at(300))),
            }),
            ..Default::default()
        };
        let undated = K8sEvent {
            reason: None,
            ..Default::default()
        };

        let events = format_events(&[undated, legacy, series], at(400));

        assert_eq!(
            events,
            vec![
                PodEvent {
                    reason: "BackOff".into(),
                    message: "Back-off restarting failed container".into(),
                    count: 4,
                    age: format_age(100),
                    tone: BadgeTone::Warning,
                },
                PodEvent {
                    reason: "Pulled".into(),
                    message: "Container image already present".into(),
                    count: 2,
                    age: format_age(300),
                    tone: BadgeTone::Good,
                },
                PodEvent {
                    reason: "Unknown".into(),
                    message: String::new(),
                    count: 1,
                    age: "unknown".into(),
                    tone: BadgeTone::Unknown,
                },
            ],
            "newest first by series time, then legacy time; undated last"
        );
    }

    #[test]
    fn a_managed_fields_entry_pretty_prints_what_it_owns() {
        let ownership = serde_json::json!({ "f:metadata": { "f:labels": { "f:app": {} } } });
        let entry = managed_field_entry(&ManagedFieldsEntry {
            manager: Some("kubectl-client-side-apply".into()),
            operation: Some("Update".into()),
            fields_v1: Some(FieldsV1(ownership.clone())),
            ..Default::default()
        });

        assert_eq!(
            entry,
            ManagedFieldEntry {
                manager: "kubectl-client-side-apply".into(),
                operation: "Update".into(),
                fields_json: serde_json::to_string_pretty(&ownership).unwrap(),
            }
        );
    }

    #[test]
    fn a_bare_managed_fields_entry_is_kept_and_says_what_is_missing() {
        let entry = managed_field_entry(&ManagedFieldsEntry::default());

        assert_eq!(entry.manager, "unknown manager");
        assert_eq!(entry.operation, "unknown operation");
        assert_eq!(entry.fields_json, "(no field ownership recorded)");
    }

    #[test]
    fn the_events_selector_pins_kind_and_uid() {
        let mut pod = rich_pod();
        pod.metadata.uid = Some("pod-uid-1".into());
        assert_eq!(
            events_selector(&pod, "staging", "api-7d9f-ftg5t"),
            "involvedObject.kind=Pod,involvedObject.namespace=staging,\
             involvedObject.name=api-7d9f-ftg5t,involvedObject.uid=pod-uid-1"
        );

        pod.metadata.uid = None;
        assert_eq!(
            events_selector(&pod, "staging", "api-7d9f-ftg5t"),
            "involvedObject.kind=Pod,involvedObject.namespace=staging,\
             involvedObject.name=api-7d9f-ftg5t",
            "a pod with no UID yet is still matched by kind and name"
        );
    }

    fn stub_panel(
        cx: &mut TestAppContext,
        state: ConnectionState,
    ) -> gpui_kit::WindowHandle<PodDetailPanel> {
        stub_panel_viewing(cx, state, DetailView::Structured)
    }

    /// The same panel, opened on a specific view - so a test can cover the
    /// `y` path (which opens straight into YAML) without a live cluster.
    fn stub_panel_viewing(
        cx: &mut TestAppContext,
        state: ConnectionState,
        view: DetailView,
    ) -> gpui_kit::WindowHandle<PodDetailPanel> {
        let connection = cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(state)));
        cx.add_window(|_window, cx| {
            let pod = PodRef {
                namespace: "staging".into(),
                name: "api-7d9f-ftg5t".into(),
            };
            let scope = PanelScope::new(
                NavTarget::pod("staging", "api-7d9f-ftg5t"),
                "kind-dev".into(),
            );
            PodDetailPanel::with_connection(pod, scope, view, connection, cx)
        })
    }

    /// `nav.show_pod_detail_yaml` asks for the panel on the YAML, so the view
    /// the panel opens on is a construction argument rather than a constant.
    /// The other half - that the same panel switches view when it is already
    /// open - is `MainWindow`'s, and is tested there.
    #[gpui_kit::test]
    async fn a_panel_can_be_built_opening_on_the_yaml(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = stub_panel_viewing(cx, ConnectionState::Connecting, DetailView::Yaml);

        window
            .update(cx, |panel, _window, _cx| {
                assert_eq!(panel.viewing, DetailView::Yaml);
            })
            .unwrap();

        // And it is a view, not a mode: the toggle still moves between the two.
        window
            .update(cx, |panel, _window, cx| {
                panel.set_view(DetailView::Structured, cx);
                assert_eq!(panel.viewing, DetailView::Structured);
            })
            .unwrap();
    }

    /// Section 4.1: the structured field list is the panel's default view, and
    /// it is the projection of the pod it holds - not a text blob.
    #[gpui_kit::test]
    async fn the_panel_renders_the_structured_field_list_by_default(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = stub_panel(cx, ConnectionState::Connecting);

        window
            .update(cx, |panel, _window, cx| {
                panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
                cx.notify();
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |panel, _window, _cx| {
                assert_eq!(panel.viewing, DetailView::Structured);
                let fields = panel.fields(Timestamp::from_second(90).unwrap());
                assert!(field(&fields, "Name").is_some());
                assert!(field(&fields, "Conditions").is_some());
                assert!(
                    matches!(
                        field(&fields, "Labels").unwrap().value,
                        PodFieldValue::Chips(_)
                    ),
                    "labels are chips, not one run-on line"
                );
                assert!(
                    matches!(
                        field(&fields, "Conditions").unwrap().value,
                        PodFieldValue::Badges(_)
                    ),
                    "conditions are badges"
                );
                assert!(
                    matches!(
                        field(&fields, "Managed Fields").unwrap().value,
                        PodFieldValue::ManagedFields(_)
                    ),
                    "managed fields render as per-manager blocks"
                );
            })
            .unwrap();
        // The render itself runs as part of the window's frame, so a panic in
        // it would fail here rather than being silently skipped.
        cx.run_until_parked();
    }

    /// Section 4.3: the toolbar toggle swaps the field list for the raw YAML
    /// and back, without leaving the panel.
    #[gpui_kit::test]
    async fn the_toolbar_toggles_between_fields_and_yaml(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = stub_panel(cx, ConnectionState::Connecting);

        window
            .update(cx, |panel, _window, cx| {
                panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
                panel.set_view(DetailView::Yaml, cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |panel, _window, _cx| {
                assert_eq!(panel.viewing, DetailView::Yaml);
                let yaml = panel.yaml().expect("a loaded pod renders as YAML");
                assert!(yaml.contains("api-7d9f-ftg5t"), "the manifest is the pod's");
            })
            .unwrap();
    }

    /// Section 2.2: switching tabs shows only that tab's fields - the other
    /// tabs' fields are gone from the rendered set, not merely reordered.
    #[gpui_kit::test]
    async fn switching_tabs_shows_only_that_tabs_fields(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = stub_panel(cx, ConnectionState::Connecting);

        window
            .update(cx, |panel, _window, cx| {
                panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
                cx.notify();
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |panel, _window, cx| {
                assert_eq!(panel.active_tab(), DetailSection::Overview);
                let all = panel.fields(Timestamp::from_second(90).unwrap());
                let overview_only: Vec<&str> = all
                    .iter()
                    .filter(|f| f.section == DetailSection::Overview)
                    .map(|f| f.label)
                    .collect();
                assert!(overview_only.contains(&"Name"));
                assert!(overview_only.contains(&"Conditions"));
                assert!(!overview_only.contains(&"Containers"));

                panel.set_active_tab(DetailSection::Containers, cx);
                let containers_only: Vec<&str> = all
                    .iter()
                    .filter(|f| f.section == panel.active_tab())
                    .map(|f| f.label)
                    .collect();
                assert!(containers_only.contains(&"Containers"));
                assert!(!containers_only.contains(&"Name"));
            })
            .unwrap();
    }

    /// Section 2.3: the tab keys are not only printed in the hint bar - real
    /// keystrokes, through the panel's key context and the bound keymap, move
    /// the active tab. `y` then toggles to YAML and back, keeping the tab.
    #[gpui_kit::test]
    async fn the_tab_keys_switch_tabs_from_the_keyboard(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            cx.bind_keys(super::panel_bindings(&KeymapConfig::default()));
        });
        let window = stub_panel(cx, ConnectionState::Connecting);
        let mut vcx = VisualTestContext::from_window(window.into(), cx);

        window
            .update(&mut vcx, |panel, window, cx| {
                panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
                panel.focus_handle.clone().focus(window, cx);
                cx.notify();
            })
            .unwrap();
        vcx.run_until_parked();

        let active_tab = |vcx: &mut VisualTestContext| {
            window
                .update(vcx, |panel, _window, _cx| panel.active_tab())
                .unwrap()
        };
        assert_eq!(active_tab(&mut vcx), DetailSection::Overview);

        // Every tab, not just one: each has its own action, and an action with
        // no `on_action` listener fails silently rather than to compile. Ends
        // back on Overview so its key is exercised from another tab.
        for (keystroke, expected) in [
            ("2", DetailSection::Containers),
            ("3", DetailSection::Volumes),
            ("4", DetailSection::Events),
            ("5", DetailSection::ManagedFields),
            ("1", DetailSection::Overview),
        ] {
            vcx.simulate_keystrokes(keystroke);
            vcx.run_until_parked();
            assert_eq!(active_tab(&mut vcx), expected, "after pressing {keystroke}");
        }

        vcx.simulate_keystrokes("3 y");
        vcx.run_until_parked();
        let view = |vcx: &mut VisualTestContext| {
            window
                .update(vcx, |panel, _window, _cx| panel.view())
                .unwrap()
        };
        assert_eq!(view(&mut vcx), DetailView::Yaml);
        vcx.simulate_keystrokes("y");
        vcx.run_until_parked();
        assert_eq!(view(&mut vcx), DetailView::Structured);
        assert_eq!(
            active_tab(&mut vcx),
            DetailSection::Volumes,
            "returning from YAML lands on the tab the user left"
        );
    }

    /// Every panel shortcut is a registry command gated to this panel's key
    /// context, so it has a palette entry and a keymap id - and a
    /// `keymap.toml` override rebinds the real key.
    #[gpui_kit::test]
    async fn the_panel_shortcuts_are_context_gated_commands(cx: &mut TestAppContext) {
        let mut registry = CommandRegistry::new();
        super::register_commands(&mut registry);
        let commands: Vec<_> = registry.iter().collect();
        assert_eq!(commands.len(), 6);
        assert!(
            commands
                .iter()
                .all(|command| command.context == Some(super::PANEL_KEY_CONTEXT)
                    && command.menu.is_none()),
            "panel shortcuts are panel-scoped and stay out of the menu bar"
        );
        assert!(registry.available(&[]).is_empty());
        assert_eq!(registry.available(&[super::PANEL_KEY_CONTEXT]).len(), 6);

        let mut keymap = KeymapConfig::default();
        keymap
            .bindings
            .insert("pod_detail.tab_events".into(), "e".into());
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            cx.bind_keys(super::panel_bindings(&keymap));
        });
        let window = stub_panel(cx, ConnectionState::Connecting);
        let mut vcx = VisualTestContext::from_window(window.into(), cx);
        window
            .update(&mut vcx, |panel, window, cx| {
                panel.state = PodDetailState::Loaded(Box::new(rich_pod()), Ok(Vec::new()));
                panel.focus_handle.clone().focus(window, cx);
                cx.notify();
            })
            .unwrap();
        vcx.run_until_parked();

        vcx.simulate_keystrokes("e");
        vcx.run_until_parked();
        assert_eq!(
            window
                .update(&mut vcx, |panel, _window, _cx| panel.active_tab())
                .unwrap(),
            DetailSection::Events,
            "the override key reaches the Events tab"
        );
    }

    /// Section 4.4: a pod that is gone is reported as gone - its own state, not
    /// an error, and not a closed panel.
    #[gpui_kit::test]
    async fn a_missing_pod_shows_that_it_no_longer_exists(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
        });
        let window = stub_panel(cx, ConnectionState::Connecting);

        window
            .update(cx, |panel, _window, cx| {
                panel.state = PodDetailState::NotFound;
                cx.notify();
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |panel, _window, _cx| {
                assert!(matches!(panel.state, PodDetailState::NotFound));
                assert!(panel.pod().is_none());
                assert!(panel.fields(Timestamp::now()).is_empty());
            })
            .unwrap();
        cx.run_until_parked();
    }

    /// The fetch tells a 404 apart from every other failure, against a real
    /// (fixture) client - one that serves a pod, and 404s for anything else.
    #[test]
    fn fetch_distinguishes_a_missing_pod_from_other_failures() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (addr, _server) = runtime.block_on(spawn_fixture_api_server());

        runtime.block_on(async {
            let client = kube::Client::try_from(kube::Config::new(
                format!("http://{addr}").parse().unwrap(),
            ))
            .expect("build client");

            let found = fetch_pod(client.clone(), "staging".into(), "present".into())
                .await
                .expect("a 200 is not a failure");
            match found {
                PodFetch::Found(pod, events) => {
                    assert_eq!(pod.metadata.name.as_deref(), Some("present"));
                    let events = events.expect("the server lists this pod's events");
                    assert_eq!(events.len(), 1);
                    assert_eq!(events[0].reason.as_deref(), Some("Scheduled"));
                }
                PodFetch::NotFound => panic!("the server serves this pod"),
            }

            // A pod readable by a user who may not list events still loads;
            // the events failure is carried alongside it, not swallowed.
            let forbidden = fetch_pod(client.clone(), "staging".into(), "events-forbidden".into())
                .await
                .expect("an events failure does not fail the pod");
            match forbidden {
                PodFetch::Found(pod, events) => {
                    assert_eq!(pod.metadata.name.as_deref(), Some("events-forbidden"));
                    assert!(events.is_err(), "the 403 is reported, not an empty list");
                }
                PodFetch::NotFound => panic!("the server serves this pod"),
            }

            let missing = fetch_pod(client, "staging".into(), "gone".into())
                .await
                .expect("a 404 is a state, not a failure");
            assert!(matches!(missing, PodFetch::NotFound));
        });
    }

    /// A fixture API server serving one pod by name and 404ing every other
    /// name - the smallest thing that exercises `fetch_pod`'s error split.
    async fn spawn_fixture_api_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 4096];
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    // Only the request line: a header could name the pod too.
                    let request_line = request.lines().next().unwrap_or_default();
                    let (status, body) = if request_line.contains("/events?") {
                        if request_line.contains("events-forbidden") {
                            let status = serde_json::json!({
                                "kind": "Status",
                                "apiVersion": "v1",
                                "status": "Failure",
                                "message": "events is forbidden",
                                "reason": "Forbidden",
                                "code": 403,
                            });
                            ("403 Forbidden", status.to_string())
                        } else {
                            let events = serde_json::json!({
                                "apiVersion": "v1",
                                "kind": "EventList",
                                "metadata": {},
                                "items": [{
                                    "metadata": { "name": "present.1", "namespace": "staging" },
                                    "involvedObject": { "kind": "Pod", "name": "present" },
                                    "reason": "Scheduled",
                                    "type": "Normal",
                                }],
                            });
                            ("200 OK", events.to_string())
                        }
                    } else if let Some(name) = ["present", "events-forbidden"]
                        .into_iter()
                        .find(|name| request_line.contains(&format!("/pods/{name} ")))
                    {
                        let pod = serde_json::json!({
                            "apiVersion": "v1",
                            "kind": "Pod",
                            "metadata": { "name": name, "namespace": "staging" },
                        });
                        ("200 OK", pod.to_string())
                    } else {
                        let status = serde_json::json!({
                            "kind": "Status",
                            "apiVersion": "v1",
                            "status": "Failure",
                            "message": "pods \"gone\" not found",
                            "reason": "NotFound",
                            "code": 404,
                        });
                        ("404 Not Found", status.to_string())
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                });
            }
        });
        (addr, handle)
    }
}
