//! The generic Status section (`resource-status-message` D3) for a kind with
//! no section of its own - a CRD, Flux's `HelmRelease` and `Kustomization`
//! among them - read from the object's raw `status.conditions`, since such a
//! kind has no typed form: its Ready status, the one message its conditions
//! read as, and every condition as a badge.

use super::super::model::{ObjectField, ObjectSection};
use super::common::{condition_badges, json_condition_pairs, message_field, ready_field};
use crate::k8s::resource::status_tone;
use kube::api::DynamicObject;

/// `object`'s Status section, or `None` if it reports no conditions - it then
/// shows Overview only, as any kind without sections does.
pub(super) fn status(object: &DynamicObject) -> Option<Vec<ObjectSection>> {
    let conditions = status_tone::conditions_of(&object.data);
    if conditions.is_empty() {
        return None;
    }
    let mut fields: Vec<ObjectField> = ready_field(conditions).into_iter().collect();
    fields.extend(message_field(conditions));
    // An arbitrary condition's polarity can't be known, so each reads as
    // good news when it holds - the common case, `Ready` among them.
    if let Some(badges) = condition_badges(json_condition_pairs(conditions), |_| false) {
        fields.push(ObjectField::new("Conditions", badges));
    }
    Some(vec![ObjectSection::new("Status", fields)])
}
