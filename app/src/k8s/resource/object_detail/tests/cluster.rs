//! The Namespace section (`standard-resource-panels` 3.3). The Node's is in
//! `sections`.

use super::fixtures::{kind, object};
use super::sections::field;
use crate::k8s::resource::object_detail::model::FieldValue;
use crate::k8s::resource::object_detail::sections::sections_for;
use crate::ui::detail::BadgeTone;
use serde_json::json;

/// A Namespace shows its phase and conditions. Every namespace condition
/// reports something stalling its deletion, so each one that holds is a
/// warning.
#[test]
fn a_namespace_shows_its_phase_and_conditions() {
    let namespace = object(json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": { "name": "staging" },
        "status": {
            "phase": "Terminating",
            "conditions": [
                { "type": "NamespaceContentRemaining", "status": "True" },
                { "type": "NamespaceDeletionDiscoveryFailure", "status": "False" },
            ],
        },
    }));

    let sections = sections_for(&kind("", "v1", "Namespace", false), &namespace);

    assert_eq!(sections[0].title, "Namespace");
    assert_eq!(field(&sections, "Phase").value.text(), "Terminating");
    assert_eq!(
        field(&sections, "Conditions").value,
        FieldValue::Badges(vec![
            ("NamespaceContentRemaining".into(), BadgeTone::Warning),
            ("NamespaceDeletionDiscoveryFailure".into(), BadgeTone::Good),
        ])
    );
}
