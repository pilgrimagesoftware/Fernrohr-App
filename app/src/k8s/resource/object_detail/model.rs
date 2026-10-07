//! The object panel's view model: titled sections of labelled fields, each
//! value shaped so the renderer knows how to draw it. Built by `metadata` and
//! `sections` from the fetched object; no `DynamicObject` here.

use crate::k8s::object_ref::ObjectRef;
use crate::ui::detail::BadgeTone;
use crate::ui::link::GoToEntry;

/// One titled group of fields - Overview, then the kind's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSection {
    pub title: String,
    pub fields: Vec<ObjectField>,
}

impl ObjectSection {
    pub fn new(title: impl Into<String>, fields: Vec<ObjectField>) -> Self {
        Self {
            title: title.into(),
            fields,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectField {
    pub label: String,
    pub value: FieldValue,
}

impl ObjectField {
    pub fn new(label: impl Into<String>, value: FieldValue) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }

    pub fn text(label: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(label, FieldValue::Text(text.into()))
    }

    /// References to other objects; `qualified` as in pod detail - `Kind/name`
    /// for a row of mixed kinds, the bare name when the label names the kind.
    pub fn references(label: impl Into<String>, targets: Vec<ObjectRef>, qualified: bool) -> Self {
        Self::new(label, FieldValue::References { targets, qualified })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    References {
        targets: Vec<ObjectRef>,
        qualified: bool,
    },
    /// One chip each - labels, annotations, capacities.
    Chips(Vec<String>),
    /// Labels or annotations, one chip each, a large value shortened to its
    /// preview with the full value in a tooltip (`ui::detail::metadata_chips`).
    Metadata(Vec<(String, String)>),
    /// One colored badge each - conditions.
    Badges(Vec<(String, BadgeTone)>),
    /// One line each.
    Lines(Vec<String>),
    /// Web addresses, each a link that opens in the browser - an Ingress's
    /// hosts (#157).
    Urls(Vec<String>),
    /// Key/value pairs with room for a long value - a ConfigMap's data.
    KeyValues(Vec<(String, String)>),
    /// A Secret's keys with their sizes, each revealable one at a time - the
    /// values themselves are never here.
    SecretKeys {
        secret: ObjectRef,
        keys: Vec<(String, usize)>,
    },
}

impl FieldValue {
    /// The single-line text this value reads as, for tests that assert on a
    /// value rather than a rendered element.
    #[cfg(test)]
    pub fn text(&self) -> String {
        match self {
            FieldValue::Text(text) => text.clone(),
            FieldValue::References { targets, qualified } => targets
                .iter()
                .map(|target| {
                    if *qualified {
                        target.qualified_name()
                    } else {
                        target.name.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", "),
            FieldValue::Chips(items) | FieldValue::Lines(items) | FieldValue::Urls(items) => {
                items.join(", ")
            }
            FieldValue::Badges(badges) => badges
                .iter()
                .map(|(text, _)| text.clone())
                .collect::<Vec<_>>()
                .join(", "),
            FieldValue::KeyValues(pairs) | FieldValue::Metadata(pairs) => pairs
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(", "),
            FieldValue::SecretKeys { keys, .. } => keys
                .iter()
                .map(|(key, size)| format!("{key}: {size} bytes"))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

/// Every reference the sections show, with its field - what the "Go to…"
/// picker lists. Unfollowable ones are filtered by the caller.
pub fn go_to_entries(sections: &[ObjectSection]) -> Vec<GoToEntry> {
    sections
        .iter()
        .flat_map(|section| &section.fields)
        .flat_map(|field| match &field.value {
            FieldValue::References { targets, .. } => targets
                .iter()
                .map(|target| GoToEntry::new(target.clone(), field.label.clone()))
                .collect(),
            FieldValue::Text(_)
            | FieldValue::Chips(_)
            | FieldValue::Metadata(_)
            | FieldValue::Badges(_)
            | FieldValue::Lines(_)
            | FieldValue::Urls(_)
            | FieldValue::KeyValues(_)
            | FieldValue::SecretKeys { .. } => Vec::new(),
        })
        .collect()
}
