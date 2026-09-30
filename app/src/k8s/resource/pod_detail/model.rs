//! The view model the projection produces and the panel draws: fields, their
//! tab, and the shaped values (chips, badges, container cards, ...) a row can
//! hold. No `Pod` in sight - see `fields` for where one becomes these.

use crate::k8s::object_ref::ObjectRef;

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

    pub(super) fn label(self) -> &'static str {
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
    /// Values naming other objects - the pod's namespace, node, service
    /// account, owners, image pull secrets. One entry per object, never one
    /// joined string; whether each is followable is decided at render time by
    /// `nav::viewer_for`, not here.
    ///
    /// `qualified` is whether each reads as `Kind/name` rather than the bare
    /// name: a row whose label already says the kind ("Node") reads better
    /// without it, a row of mixed kinds ("Controlled By") needs it.
    References {
        targets: Vec<ObjectRef>,
        qualified: bool,
    },
    /// Key=value pairs, one chip each.
    Chips(Vec<String>),
    /// Conditions, one badge each.
    Badges(Vec<ConditionBadge>),
    /// Rows behind a disclosure, collapsed by default so a long list does not
    /// push the fields the user came for off screen.
    Collapsed(Vec<String>),
    /// One row per volume, always shown - the Volumes tab exists for these,
    /// so a Show/Hide toggle would just be an extra click to see them.
    Volumes(Vec<VolumeRow>),
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
            PodFieldValue::Text(text) => text.clone(),
            PodFieldValue::References { targets, qualified } => targets
                .iter()
                .map(|target| reference_text(target, *qualified))
                .collect::<Vec<_>>()
                .join(", "),
            PodFieldValue::Chips(chips) => chips.join(", "),
            PodFieldValue::Badges(badges) => badges
                .iter()
                .map(|badge| format!("{}={}", badge.condition, badge.status))
                .collect::<Vec<_>>()
                .join(", "),
            PodFieldValue::Collapsed(rows) => rows.join(", "),
            PodFieldValue::Volumes(volumes) => volumes
                .iter()
                .map(VolumeRow::text)
                .collect::<Vec<_>>()
                .join(", "),
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
    /// The ConfigMaps and Secrets this container reads environment from,
    /// one per object - see `references::env_sources`.
    pub env_sources: Vec<ObjectRef>,
}

/// One volume: its name, what kind of source backs it, and the object behind
/// that source when there is one. The referenced object's name lives in
/// `references`, not in the text, so it can be drawn as a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeRow {
    pub name: String,
    /// `ConfigMap`, `Secret`, `PersistentVolumeClaim`, `Projected`,
    /// `HostPath`, `EmptyDir`, or `Other`.
    pub source: &'static str,
    /// Text that belongs after the source but is not an object - a host
    /// path's directory.
    pub detail: Option<String>,
    pub references: Vec<ObjectRef>,
}

impl VolumeRow {
    /// `name: Source: target, ...` - the single line this row reads as.
    #[cfg(test)]
    pub fn text(&self) -> String {
        let mut tail: Vec<String> = self.detail.iter().cloned().collect();
        tail.extend(self.references.iter().map(|target| target.name.clone()));
        if tail.is_empty() {
            format!("{}: {}", self.name, self.source)
        } else {
            format!("{}: {}: {}", self.name, self.source, tail.join(", "))
        }
    }
}

/// How a reference reads as text: `Kind/name` when `qualified`, the bare name
/// otherwise.
pub(crate) fn reference_text(target: &ObjectRef, qualified: bool) -> String {
    if qualified {
        target.qualified_name()
    } else {
        target.name.clone()
    }
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

pub(super) fn condition_tone(status: &str) -> BadgeTone {
    match status {
        "True" => BadgeTone::Good,
        "False" => BadgeTone::Warning,
        _ => BadgeTone::Unknown,
    }
}

pub(super) fn chip(key: &str, value: &str) -> String {
    format!("{key}={value}")
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
