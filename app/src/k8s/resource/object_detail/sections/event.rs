//! An Event's section (`events-browser` 2.3): what happened, how often, who
//! reported it, and the objects it is about. Every time, count and reporter
//! reads through `k8s::resource::events`, which falls back to the
//! `events.k8s.io/v1` fields when the legacy ones are empty.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use super::common::non_empty;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::events;
use crate::k8s::resource::pods::format_age;
use crate::ui::detail::BadgeTone;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::{Event, ObjectReference};

pub(super) fn event(event: &Event) -> Vec<ObjectSection> {
    event_at(event, Timestamp::now())
}

/// [`event`], its ages read against `now`.
pub(super) fn event_at(event: &Event, now: Timestamp) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    if let Some(type_) = events::text(&event.type_) {
        let tone = match type_.as_str() {
            "Warning" => BadgeTone::Warning,
            _ => BadgeTone::Good,
        };
        fields.push(ObjectField::new(
            "Type",
            FieldValue::Badges(vec![(type_, tone)]),
        ));
    }
    if let Some(reason) = events::text(&event.reason) {
        fields.push(ObjectField::text("Reason", reason));
    }
    if let Some(message) = events::text(&event.message) {
        fields.push(ObjectField::text("Message", message));
    }
    fields.push(ObjectField::text(
        "Count",
        events::event_count(event).to_string(),
    ));
    let when = |time: Timestamp| {
        let age = format_age(now.duration_since(time).as_secs());
        format!("{} ({age} ago)", time.strftime("%Y-%m-%d %H:%M:%S UTC"))
    };
    if let Some(first) = events::first_seen(event) {
        fields.push(ObjectField::text("First Seen", when(first)));
    }
    if let Some(last) = events::event_time(event) {
        fields.push(ObjectField::text("Last Seen", when(last)));
    }
    if let Some(reporter) = events::reporter(event) {
        fields.push(ObjectField::text("Reporting Component", reporter));
    }
    if let Some(instance) = events::reporting_instance(event) {
        fields.push(ObjectField::text("Reporting Instance", instance));
    }
    if let Some(action) = non_empty(event.action.as_deref()) {
        fields.push(ObjectField::text("Action", action));
    }
    if let Some(involved) = object_ref(&event.involved_object) {
        fields.push(ObjectField::references(
            "Involved Object",
            vec![involved],
            true,
        ));
    }
    if let Some(related) = event.related.as_ref().and_then(object_ref) {
        fields.push(ObjectField::references(
            "Related Object",
            vec![related],
            true,
        ));
    }
    vec![ObjectSection::new("Event", fields)]
}

/// `reference` as a followable object, or `None` when it names no kind or name.
fn object_ref(reference: &ObjectReference) -> Option<ObjectRef> {
    let kind = non_empty(reference.kind.as_deref())?;
    let name = non_empty(reference.name.as_deref())?;
    Some(ObjectRef::from_api_version(
        reference.api_version.as_deref().unwrap_or_default(),
        kind,
        events::text(&reference.namespace),
        name,
    ))
}

#[cfg(test)]
mod tests;
