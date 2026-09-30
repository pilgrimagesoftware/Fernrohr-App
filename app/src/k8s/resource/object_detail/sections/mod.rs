//! The sections a kind gets after Overview, projected from the object's typed
//! `k8s-openapi` form. A kind not listed here - or an object that doesn't
//! deserialize as its kind - gets Overview only.
//!
//! This dispatch is where a kind gains a structured viewer; nothing that shows
//! a reference to it needs to change.

use super::model::ObjectSection;
use crate::k8s::cluster::discovery::DiscoveredKind;
use kube::api::DynamicObject;

/// `object`'s kind-specific sections, in display order. None yet: section 6
/// of `resource-links` adds them per kind.
pub(super) fn sections_for(_kind: &DiscoveredKind, _object: &DynamicObject) -> Vec<ObjectSection> {
    Vec::new()
}
