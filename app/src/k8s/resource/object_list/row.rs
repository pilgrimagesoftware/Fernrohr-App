//! One listed object, reduced to what a list row shows. Built once per watch
//! event, when the store stores it, never per frame - the one place a kind's
//! per-kind cells (`standard-resource-panels` section 2) are computed too.

use super::columns::{Cell, KindColumns};
use jiff::Timestamp;
use kube::api::DynamicObject;

/// The base columns of any kind's list - Name, Namespace (namespaced kinds only)
/// and Age - plus the uid the store keys rows by.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectRow {
    pub uid: String,
    pub name: String,
    /// `None` for a cluster-scoped object.
    pub namespace: Option<String>,
    /// When the object was created; Age is measured from it at render time.
    pub created: Option<Timestamp>,
    /// The kind's own columns' cells, in [`KindColumns::columns`] order. Empty
    /// for a kind with base columns only.
    pub cells: Vec<Cell>,
}

impl ObjectRow {
    /// `object`'s row, with `columns`' cells if its kind has its own.
    pub fn new(object: &DynamicObject, columns: Option<&KindColumns>) -> Self {
        let meta = &object.metadata;
        Self {
            uid: meta.uid.clone().unwrap_or_default(),
            name: meta.name.clone().unwrap_or_default(),
            namespace: meta.namespace.clone(),
            created: meta.creation_timestamp.as_ref().map(|time| time.0),
            cells: columns
                .map(|columns| columns.cells_for(object))
                .unwrap_or_default(),
        }
    }

    /// Seconds since creation at `now`, as the Pods table's Age measures it.
    pub fn age_secs(&self, now: Timestamp) -> i64 {
        self.created
            .map(|created| now.duration_since(created).as_secs_f64() as i64)
            .unwrap_or_default()
    }
}
