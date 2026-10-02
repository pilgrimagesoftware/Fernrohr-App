//! One event as the events browser shows it, read once from the typed `Event`
//! with the `events.k8s.io/v1` fallbacks `k8s::resource::events` applies.

use crate::k8s::resource::events;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Event as K8sEvent;

/// The object an event is about, as its `involvedObject` names it - enough to
/// link to that object's detail panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvolvedRef {
    pub kind: String,
    pub api_version: String,
    pub namespace: Option<String>,
    pub name: String,
}

/// One event's columns: last seen, type, reason, involved object, message,
/// count and source - plus the uid the store keys rows by and the namespace
/// the panel's scope filters on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRow {
    pub uid: String,
    pub namespace: Option<String>,
    pub last_seen: Option<Timestamp>,
    /// `Warning`, `Normal`, or whatever the reporter set; `None` when unset.
    pub type_: Option<String>,
    pub reason: String,
    pub involved: InvolvedRef,
    pub message: String,
    pub count: i32,
    /// The reporting component, with its instance when known.
    pub source: String,
}

impl EventRow {
    pub fn new(event: &K8sEvent) -> Self {
        let meta = &event.metadata;
        let involved = &event.involved_object;
        let source = match (events::reporter(event), events::reporting_instance(event)) {
            (Some(component), Some(instance)) => format!("{component} ({instance})"),
            (Some(component), None) => component,
            (None, Some(instance)) => instance,
            (None, None) => String::new(),
        };
        Self {
            uid: meta.uid.clone().unwrap_or_default(),
            namespace: meta.namespace.clone(),
            last_seen: events::event_time(event),
            type_: events::text(&event.type_),
            reason: events::text(&event.reason).unwrap_or_default(),
            involved: InvolvedRef {
                kind: involved.kind.clone().unwrap_or_default(),
                api_version: involved.api_version.clone().unwrap_or_default(),
                namespace: events::text(&involved.namespace),
                name: involved.name.clone().unwrap_or_default(),
            },
            message: events::text(&event.message).unwrap_or_default(),
            count: events::event_count(event),
            source,
        }
    }

    /// The involved object as the table writes it: `Kind/name`.
    pub fn object_label(&self) -> String {
        format!("{}/{}", self.involved.kind, self.involved.name)
    }
}
