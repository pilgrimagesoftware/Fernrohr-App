//! Per-kind list columns (`standard-resource-panels` D2): the columns a
//! built-in kind's list shows beyond the base Name, Namespace and Age - what
//! `kubectl get` shows by default - and the cells that fill them.
//!
//! A kind's columns are data, not a panel: a [`KindColumns`] table pairs the
//! headers with one extractor that turns an object into all its cells. The
//! store runs the extractor once per watch event, when it stores the row, by
//! deserializing the object into its `k8s-openapi` type; nothing here runs per
//! frame. An object that doesn't deserialize as its kind gets empty cells, and
//! a kind with no table gets the base columns only.

mod config_network;
mod workloads;

use jiff::Timestamp;
use kube::api::DynamicObject;
use serde::de::DeserializeOwned;
use std::cmp::Ordering;

/// One cell's value, typed so its column sorts the way the value reads:
/// numbers numerically, ratios by their first number, ages by time.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    /// No value: the object doesn't set it, or didn't deserialize.
    Empty,
    Text(String),
    Number(i64),
    /// `ready/desired` - sorts by `ready`, then `desired`.
    Ratio(i64, i64),
    /// Time since a moment (a last schedule), shown like Age.
    Age(Timestamp),
    /// A fixed span in seconds (a finished Job's duration), shown like Age.
    Duration(i64),
}

impl Cell {
    /// `Text`, or `Empty` for an empty string - so a column of text sorts the
    /// unset ones together.
    pub fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        if text.is_empty() {
            Self::Empty
        } else {
            Self::Text(text)
        }
    }

    /// What the cell reads as, `now` being when the table last refreshed.
    pub fn display(&self, now: Timestamp) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Text(text) => text.clone(),
            Self::Number(number) => number.to_string(),
            Self::Ratio(have, want) => format!("{have}/{want}"),
            Self::Age(since) => crate::k8s::resource::pods::format_age(seconds_since(*since, now)),
            Self::Duration(secs) => crate::k8s::resource::pods::format_age(*secs),
        }
    }

    /// How `self` and `other` order, `now` as in [`Cell::display`]. Values of
    /// one variant compare by value - numerically for numbers, ratios and
    /// times - and an empty cell sorts before any value.
    pub fn compare(&self, other: &Self, now: Timestamp) -> Ordering {
        match (self, other) {
            (Self::Empty, Self::Empty) => Ordering::Equal,
            (Self::Empty, _) => Ordering::Less,
            (_, Self::Empty) => Ordering::Greater,
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
            (Self::Number(a), Self::Number(b)) => a.cmp(b),
            (Self::Ratio(a_have, a_want), Self::Ratio(b_have, b_want)) => {
                a_have.cmp(b_have).then(a_want.cmp(b_want))
            }
            (Self::Age(a), Self::Age(b)) => seconds_since(*a, now).cmp(&seconds_since(*b, now)),
            (Self::Duration(a), Self::Duration(b)) => a.cmp(b),
            // One column holds one variant; mixed ones (a running Job's live
            // Age beside a finished one's Duration) compare by elapsed time.
            (a, b) => a.seconds(now).cmp(&b.seconds(now)),
        }
    }

    /// A time-like cell's elapsed seconds, `None` for any other - the order
    /// of last resort for mixed variants.
    fn seconds(&self, now: Timestamp) -> Option<i64> {
        match self {
            Self::Age(since) => Some(seconds_since(*since, now)),
            Self::Duration(secs) => Some(*secs),
            Self::Empty | Self::Text(_) | Self::Number(_) | Self::Ratio(..) => None,
        }
    }
}

fn seconds_since(since: Timestamp, now: Timestamp) -> i64 {
    now.duration_since(since).as_secs_f64() as i64
}

/// One per-kind column's header.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnDef {
    /// The key a saved layout names the column by.
    pub id: &'static str,
    pub title: &'static str,
    /// Default width in logical pixels.
    pub width: f32,
}

/// A kind's extra columns and the extractor that fills them: `cells` returns
/// one cell per column, in order.
pub struct KindColumns {
    pub columns: &'static [ColumnDef],
    pub cells: fn(&DynamicObject) -> Vec<Cell>,
}

impl KindColumns {
    /// `object`'s cells, padded or cut to one per column - so a slip in an
    /// extractor misaligns nothing.
    pub fn cells_for(&self, object: &DynamicObject) -> Vec<Cell> {
        let mut cells = (self.cells)(object);
        cells.resize(self.columns.len(), Cell::Empty);
        cells
    }
}

/// The extra columns a `group`/`kind` list shows, or `None` for a kind that
/// shows the base columns only - every kind not in the spec's table, CRDs
/// included. Pods have their own table and never come here.
pub fn for_kind(group: &str, kind: &str) -> Option<&'static KindColumns> {
    Some(match (group, kind) {
        ("apps", "Deployment") => &workloads::DEPLOYMENT,
        ("apps", "ReplicaSet") => &workloads::REPLICA_SET,
        ("apps", "StatefulSet") => &workloads::STATEFUL_SET,
        ("apps", "DaemonSet") => &workloads::DAEMON_SET,
        ("batch", "Job") => &workloads::JOB,
        ("batch", "CronJob") => &workloads::CRON_JOB,
        ("", "ConfigMap") => &config_network::CONFIG_MAP,
        ("", "Secret") => &config_network::SECRET,
        ("", "Service") => &config_network::SERVICE,
        ("", "Endpoints") => &config_network::ENDPOINTS,
        ("networking.k8s.io", "Ingress") => &config_network::INGRESS,
        ("networking.k8s.io", "NetworkPolicy") => &config_network::NETWORK_POLICY,
        ("discovery.k8s.io", "EndpointSlice") => &config_network::ENDPOINT_SLICE,
        _ => return None,
    })
}

/// `object` deserialized as `T` and projected by `project`, or every column
/// empty if it doesn't deserialize - a malformed object never panics, it just
/// shows the base columns.
fn typed_cells<T: DeserializeOwned>(
    object: &DynamicObject,
    project: impl FnOnce(&T) -> Vec<Cell>,
) -> Vec<Cell> {
    serde_json::to_value(object)
        .ok()
        .and_then(|value| serde_json::from_value::<T>(value).ok())
        .map(|typed| project(&typed))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
