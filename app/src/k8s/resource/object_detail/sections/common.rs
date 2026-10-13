//! Projection helpers more than one kind's sections share: label selectors,
//! resource quantities, condition badges, and the status message and Ready
//! field conditions read as (`resource-status-message`).

use super::super::model::{FieldValue, ObjectField};
use crate::k8s::resource::status_tone;
use crate::ui::detail::BadgeTone;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use std::collections::BTreeMap;

/// A selector as chips: `key=value` per match label, and one
/// `key operator (values)` per match expression.
pub(super) fn selector_chips(selector: &LabelSelector) -> Vec<String> {
    let labels = selector
        .match_labels
        .iter()
        .flatten()
        .map(|(key, value)| format!("{key}={value}"));
    let expressions = selector
        .match_expressions
        .iter()
        .flatten()
        .map(|expression| {
            let values = expression.values.as_deref().unwrap_or_default().join(", ");
            if values.is_empty() {
                format!("{} {}", expression.key, expression.operator)
            } else {
                format!("{} {} ({values})", expression.key, expression.operator)
            }
        });
    labels.chain(expressions).collect()
}

/// Resource quantities as `name=quantity` chips, or `None` when there are none.
pub(super) fn quantities(map: Option<&BTreeMap<String, Quantity>>) -> Option<FieldValue> {
    let map = map.filter(|map| !map.is_empty())?;
    Some(FieldValue::Chips(
        map.iter()
            .map(|(name, quantity)| format!("{name}={}", quantity.0))
            .collect(),
    ))
}

/// Conditions as badges. Most conditions are good news when `True`; those
/// named in `negative` (`MemoryPressure`, `ReplicaFailure`, ...) are good news
/// when `False`. Anything but `True`/`False` is `Unknown`.
pub(super) fn condition_badges<'a>(
    conditions: impl IntoIterator<Item = (&'a str, &'a str)>,
    negative: impl Fn(&str) -> bool,
) -> Option<FieldValue> {
    let badges: Vec<(String, BadgeTone)> = conditions
        .into_iter()
        .map(|(condition, status)| {
            let holds = match status {
                "True" => Some(true),
                "False" => Some(false),
                _ => None,
            };
            let tone = match holds.map(|holds| holds != negative(condition)) {
                Some(true) => BadgeTone::Good,
                Some(false) => BadgeTone::Warning,
                None => BadgeTone::Unknown,
            };
            (condition.to_string(), tone)
        })
        .collect();
    (!badges.is_empty()).then_some(FieldValue::Badges(badges))
}

/// Raw JSON conditions (`status_tone::conditions_of`) as the `(type, status)`
/// pairs [`condition_badges`] takes - for a kind with no typed form, a CRD's.
/// A condition with no `type` is skipped; one with no `status` reads as
/// `Unknown`.
pub(super) fn json_condition_pairs(
    conditions: &[serde_json::Value],
) -> impl Iterator<Item = (&str, &str)> {
    conditions.iter().filter_map(|condition| {
        let type_ = condition.get("type")?.as_str()?;
        let status = condition
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        Some((type_, status))
    })
}

/// Typed conditions - a `k8s-openapi` status's `conditions` - back in their
/// JSON shape, for [`message_field`]: every kind's condition serializes with
/// the same `type`, `status`, `message` and `lastTransitionTime`.
pub(super) fn conditions_json<T: serde::Serialize>(
    conditions: Option<&Vec<T>>,
) -> Vec<serde_json::Value> {
    conditions
        .into_iter()
        .flatten()
        .filter_map(|condition| serde_json::to_value(condition).ok())
        .collect()
}

/// The Message field: the one message `conditions` read as, in its tone
/// (`status_tone::condition_message`), or `None` when none has one.
pub(super) fn message_field(conditions: &[serde_json::Value]) -> Option<ObjectField> {
    let (text, tone) = status_tone::condition_message(conditions)?;
    Some(ObjectField::new(
        "Message",
        FieldValue::Message { text, tone },
    ))
}

/// The Ready field: the `Ready` condition's status, or `None` without one.
pub(super) fn ready_field(conditions: &[serde_json::Value]) -> Option<ObjectField> {
    let status = status_tone::ready_status(conditions)?;
    Some(ObjectField::new("Ready", FieldValue::Ready(status)))
}

/// `Some(value)` unless `value` is empty.
pub(super) fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_read_from_json_conditions() {
        let conditions = serde_json::json!([
            { "type": "Ready", "status": "True" },
            { "type": "Stalled", "status": "False" },
            { "status": "True" },
            { "type": "Reconciling" },
        ]);
        let conditions = conditions.as_array().unwrap();
        assert_eq!(
            condition_badges(json_condition_pairs(conditions), |condition| {
                condition == "Stalled"
            }),
            Some(FieldValue::Badges(vec![
                ("Ready".into(), BadgeTone::Good),
                ("Stalled".into(), BadgeTone::Good),
                ("Reconciling".into(), BadgeTone::Unknown),
            ]))
        );
    }
}
