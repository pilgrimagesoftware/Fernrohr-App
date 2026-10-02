//! One listed object, reduced to what a list row shows. Built once per watch
//! event, when the store stores it, never per frame - the one place a kind's
//! per-kind cells (`standard-resource-panels` section 2) will be computed too.

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
}

impl ObjectRow {
    pub fn new(object: &DynamicObject) -> Self {
        let meta = &object.metadata;
        Self {
            uid: meta.uid.clone().unwrap_or_default(),
            name: meta.name.clone().unwrap_or_default(),
            namespace: meta.namespace.clone(),
            created: meta.creation_timestamp.as_ref().map(|time| time.0),
        }
    }

    // UNWIRED: `ObjectListPanel` (`standard-resource-panels` 1.3) is the first
    // caller; until then only tests use it.
    #[allow(dead_code)]
    /// Seconds since creation at `now`, as the Pods table's Age measures it.
    pub fn age_secs(&self, now: Timestamp) -> i64 {
        self.created
            .map(|created| now.duration_since(created).as_secs_f64() as i64)
            .unwrap_or_default()
    }
}
