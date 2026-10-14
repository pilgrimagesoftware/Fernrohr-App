//! Reading `status.conditions` from any object, built-in or CRD
//! (`resource-status-message` D1): the one status message a list's Message
//! cell and a detail panel's Message field both show, and the `Ready`
//! condition's status. Read from the raw JSON, not a typed `Condition` - a
//! CRD such as Flux's `HelmRelease` has no generated type, and the condition
//! shape (`type`, `status`, `message`, `lastTransitionTime`) is the same for
//! every kind.

use crate::ui::style::Tone;
use jiff::Timestamp;
use serde_json::Value;
use std::fmt;

/// A condition's status. Ordered as a Ready column sorts ascending: failures
/// first, then the undecided, then the healthy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConditionStatus {
    False,
    Unknown,
    True,
}

impl ConditionStatus {
    /// `True` and `False` as written; anything else - `Unknown`, a missing
    /// status, a non-standard value - is `Unknown`.
    pub fn parse(status: Option<&str>) -> Self {
        match status {
            Some("True") => Self::True,
            Some("False") => Self::False,
            _ => Self::Unknown,
        }
    }

    /// The tone of a `Ready` condition with this status: true is good, false
    /// bad, anything else a warning.
    pub fn ready_tone(self) -> Tone {
        match self {
            Self::True => Tone::Good,
            Self::False => Tone::Bad,
            Self::Unknown => Tone::Warning,
        }
    }
}

impl fmt::Display for ConditionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::True => "True",
            Self::False => "False",
            Self::Unknown => "Unknown",
        })
    }
}

/// The `status.conditions` of `data` - an object's JSON, as
/// `DynamicObject::data` holds it - or nothing if it reports none.
pub fn conditions_of(data: &Value) -> &[Value] {
    data.pointer("/status/conditions")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The status of the `Ready` condition among `conditions`, `None` if there is
/// no `Ready` condition.
pub fn ready_status(conditions: &[Value]) -> Option<ConditionStatus> {
    ready_condition(conditions).map(|ready| ConditionStatus::parse(field(ready, "status")))
}

/// The one message `conditions` read as, with its tone, or `None` when none
/// qualifies:
///
/// - the `Ready` condition's message, toned by its status
///   ([`ConditionStatus::ready_tone`]) - and nothing if that message is empty,
///   rather than another condition's;
/// - with no `Ready` condition, the message of the most recently transitioned
///   condition that has one, [`Tone::Neutral`] - an arbitrary condition's
///   polarity can't be known, so it implies neither good nor bad. A condition
///   whose `lastTransitionTime` is missing or doesn't parse counts as older
///   than any that parses; among equals the later one in the list wins.
pub fn condition_message(conditions: &[Value]) -> Option<(String, Tone)> {
    if let Some(ready) = ready_condition(conditions) {
        let message = non_empty(field(ready, "message"))?;
        let tone = ConditionStatus::parse(field(ready, "status")).ready_tone();
        return Some((message.to_string(), tone));
    }
    conditions
        .iter()
        .filter_map(|condition| Some((non_empty(field(condition, "message"))?, condition)))
        .max_by_key(|(_, condition)| {
            field(condition, "lastTransitionTime").and_then(|time| time.parse::<Timestamp>().ok())
        })
        .map(|(message, _)| (message.to_string(), Tone::Neutral))
}

fn ready_condition(conditions: &[Value]) -> Option<&Value> {
    conditions
        .iter()
        .find(|condition| field(condition, "type") == Some("Ready"))
}

fn field<'a>(condition: &'a Value, name: &str) -> Option<&'a str> {
    condition.get(name).and_then(Value::as_str)
}

fn non_empty(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.is_empty())
}
