//! The events browser's columns: identity, title, default width, the text a cell
//! shows, and how two rows compare on it.

use super::row::EventRow;
use crate::k8s::resource::pods::format_age;
use jiff::Timestamp;
use std::cmp::Ordering;

/// One column of the events table. A closed enum, so a new column fails to
/// compile wherever it isn't handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventColumn {
    LastSeen,
    Type,
    Reason,
    Object,
    Message,
    Count,
    Source,
}

impl EventColumn {
    pub const DEFAULT_ORDER: [EventColumn; 7] = [
        EventColumn::LastSeen,
        EventColumn::Type,
        EventColumn::Reason,
        EventColumn::Object,
        EventColumn::Message,
        EventColumn::Count,
        EventColumn::Source,
    ];

    pub fn id(self) -> &'static str {
        match self {
            EventColumn::LastSeen => "last_seen",
            EventColumn::Type => "type",
            EventColumn::Reason => "reason",
            EventColumn::Object => "object",
            EventColumn::Message => "message",
            EventColumn::Count => "count",
            EventColumn::Source => "source",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            EventColumn::LastSeen => "Last Seen",
            EventColumn::Type => "Type",
            EventColumn::Reason => "Reason",
            EventColumn::Object => "Object",
            EventColumn::Message => "Message",
            EventColumn::Count => "Count",
            EventColumn::Source => "Source",
        }
    }

    pub fn default_width(self) -> f32 {
        match self {
            EventColumn::LastSeen => 90.,
            EventColumn::Type => 90.,
            EventColumn::Reason => 150.,
            EventColumn::Object => 240.,
            EventColumn::Message => 420.,
            EventColumn::Count => 70.,
            EventColumn::Source => 180.,
        }
    }

    /// The column an id names, if any - a saved sort from an older build may
    /// name one this build doesn't have.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::DEFAULT_ORDER.into_iter().find(|col| col.id() == id)
    }

    /// The text `row` shows in this column, its age read against `now`.
    pub fn text(self, row: &EventRow, now: Timestamp) -> String {
        match self {
            EventColumn::LastSeen => row
                .last_seen
                .map(|time| format_age(now.duration_since(time).as_secs()))
                .unwrap_or_else(|| "unknown".to_string()),
            EventColumn::Type => row.type_.clone().unwrap_or_default(),
            EventColumn::Reason => row.reason.clone(),
            EventColumn::Object => row.object_label(),
            EventColumn::Message => row.message.clone(),
            EventColumn::Count => row.count.to_string(),
            EventColumn::Source => row.source.clone(),
        }
    }

    /// How `a` and `b` compare on this column, ascending. Last Seen ascends by
    /// age - newest first - with an undated event last, so the default sort
    /// reads top-down from what just happened.
    pub fn compare(self, a: &EventRow, b: &EventRow) -> Ordering {
        match self {
            EventColumn::LastSeen => match (a.last_seen, b.last_seen) {
                (Some(a), Some(b)) => b.cmp(&a),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            EventColumn::Type => a.type_.cmp(&b.type_),
            EventColumn::Reason => a.reason.cmp(&b.reason),
            EventColumn::Object => a.object_label().cmp(&b.object_label()),
            EventColumn::Message => a.message.cmp(&b.message),
            EventColumn::Count => a.count.cmp(&b.count),
            EventColumn::Source => a.source.cmp(&b.source),
        }
    }
}
