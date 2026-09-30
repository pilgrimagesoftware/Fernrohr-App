//! The Overview section every kind gets (`resource-links` 5.3).

use super::fixtures::{nodes, object, owned_replica_set, replica_sets, target};
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::metadata::overview;
use crate::k8s::resource::object_detail::model::{FieldValue, ObjectField};
use jiff::Timestamp;

fn field<'a>(fields: &'a [ObjectField], label: &str) -> &'a ObjectField {
    fields
        .iter()
        .find(|field| field.label == label)
        .unwrap_or_else(|| panic!("no {label} row"))
}

#[test]
fn overview_rows_are_projected_in_order() {
    let section = overview(
        &owned_replica_set(),
        &target(replica_sets(), Some("staging"), "web-7d9f"),
        Timestamp::from_second(90).unwrap(),
    );

    assert_eq!(section.title, "Overview");
    let labels: Vec<&str> = section.fields.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Created",
            "Name",
            "Namespace",
            "Kind",
            "Labels",
            "Annotations",
            "Controlled By"
        ]
    );
    assert_eq!(
        field(&section.fields, "Created").value.text(),
        "1m (1970-01-01T00:00:00Z)"
    );
    assert_eq!(
        field(&section.fields, "Kind").value.text(),
        "ReplicaSet (apps/v1)"
    );
    assert_eq!(
        field(&section.fields, "Labels").value,
        FieldValue::Chips(vec!["app=web".into()])
    );
}

/// 5.3: the namespace and each owner are references - a ReplicaSet's
/// Deployment owner qualified by its kind.
#[test]
fn namespace_and_owners_are_references() {
    let section = overview(
        &owned_replica_set(),
        &target(replica_sets(), Some("staging"), "web-7d9f"),
        Timestamp::from_second(0).unwrap(),
    );

    assert_eq!(
        field(&section.fields, "Namespace").value,
        FieldValue::References {
            targets: vec![ObjectRef::cluster_scoped("", "Namespace", "staging")],
            qualified: false,
        }
    );
    assert_eq!(
        field(&section.fields, "Controlled By").value,
        FieldValue::References {
            targets: vec![ObjectRef::namespaced(
                "apps",
                "Deployment",
                "staging",
                "web"
            )],
            qualified: true,
        }
    );
}

/// A cluster-scoped object has no Namespace row, and an object with no owners
/// has no Controlled By row - absent, not blank.
#[test]
fn absent_rows_are_left_out() {
    let node = object(serde_json::json!({
        "apiVersion": "v1",
        "kind": "Node",
        "metadata": { "name": "node-a" },
    }));
    let section = overview(
        &node,
        &target(nodes(), None, "node-a"),
        Timestamp::from_second(0).unwrap(),
    );
    let labels: Vec<&str> = section.fields.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(labels, vec!["Name", "Kind"]);
}
