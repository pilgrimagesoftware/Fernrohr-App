//! Projection helpers more than one kind's sections share: label selectors,
//! resource quantities, and condition badges.

use super::super::model::FieldValue;
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

/// `Some(value)` unless `value` is empty.
pub(super) fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}
