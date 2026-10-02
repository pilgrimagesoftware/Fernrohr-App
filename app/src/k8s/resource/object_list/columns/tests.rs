//! `standard-resource-panels` 2.1: cells sort the way their values read, and a
//! kind's extractor always yields one cell per column.

use super::{Cell, ColumnDef, KindColumns, typed_cells};
use jiff::{SignedDuration, Timestamp};
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::DynamicObject;
use serde_json::json;
use std::cmp::Ordering;

fn now() -> Timestamp {
    "2026-10-02T12:00:00Z".parse().unwrap()
}

/// `cells` sorted ascending by [`Cell::compare`].
fn sorted(mut cells: Vec<Cell>) -> Vec<Cell> {
    let now = now();
    cells.sort_by(|a, b| a.compare(b, now));
    cells
}

#[test]
fn numbers_sort_numerically_not_as_text() {
    assert_eq!(
        sorted(vec![Cell::Number(10), Cell::Number(9), Cell::Number(100)]),
        vec![Cell::Number(9), Cell::Number(10), Cell::Number(100)],
        "as text, \"10\" and \"100\" would sort before \"9\""
    );
}

#[test]
fn ratios_sort_by_their_first_number_then_their_second() {
    assert_eq!(
        sorted(vec![
            Cell::Ratio(10, 10),
            Cell::Ratio(2, 3),
            Cell::Ratio(2, 2),
            Cell::Ratio(9, 10),
        ]),
        vec![
            Cell::Ratio(2, 2),
            Cell::Ratio(2, 3),
            Cell::Ratio(9, 10),
            Cell::Ratio(10, 10),
        ]
    );
    assert_eq!(Cell::Ratio(2, 3).display(now()), "2/3");
}

/// An empty cell - a field the object doesn't set - sorts before any value,
/// so unset rows gather at one end rather than scattering.
#[test]
fn an_empty_cell_sorts_first() {
    assert_eq!(
        sorted(vec![Cell::Number(1), Cell::Empty, Cell::Number(0)]),
        vec![Cell::Empty, Cell::Number(0), Cell::Number(1)]
    );
    assert_eq!(Cell::text(""), Cell::Empty, "an empty string is no value");
    assert_eq!(Cell::Empty.display(now()), "");
}

/// An Age cell is measured from the table's refresh, so an older moment is a
/// larger age; a Duration is a fixed span. A column mixing the two (running
/// and finished Jobs) still orders by elapsed time.
#[test]
fn ages_and_durations_sort_by_elapsed_time() {
    let now = now();
    let ago = |secs: i64| Cell::Age(now - SignedDuration::from_secs(secs));
    assert_eq!(ago(300).compare(&ago(60), now), Ordering::Greater);
    assert_eq!(ago(300).display(now), "5m");
    assert_eq!(Cell::Duration(90).display(now), "1m");
    assert_eq!(
        sorted(vec![Cell::Duration(600), ago(120), Cell::Duration(30)]),
        vec![Cell::Duration(30), ago(120), Cell::Duration(600)]
    );
}

const TWO: &[ColumnDef] = &[
    ColumnDef {
        id: "one",
        title: "One",
        width: 60.,
    },
    ColumnDef {
        id: "two",
        title: "Two",
        width: 60.,
    },
];

fn object() -> DynamicObject {
    serde_json::from_value(json!({
        "apiVersion": "v1", "kind": "ConfigMap", "metadata": { "name": "x" },
    }))
    .unwrap()
}

/// However many cells an extractor returns, a row gets exactly one per
/// column, so a slip misaligns nothing.
#[test]
fn a_kind_always_yields_one_cell_per_column() {
    let short = KindColumns {
        columns: TWO,
        cells: |_| vec![Cell::Number(1)],
    };
    let long = KindColumns {
        columns: TWO,
        cells: |_| vec![Cell::Number(1), Cell::Number(2), Cell::Number(3)],
    };
    assert_eq!(
        short.cells_for(&object()),
        vec![Cell::Number(1), Cell::Empty]
    );
    assert_eq!(
        long.cells_for(&object()),
        vec![Cell::Number(1), Cell::Number(2)]
    );
}

/// An object that doesn't deserialize as its kind yields no cells - the row
/// pads them empty - rather than panicking.
#[test]
fn a_malformed_object_yields_no_cells() {
    let malformed: DynamicObject = serde_json::from_value(json!({
        "apiVersion": "apps/v1", "kind": "Deployment", "metadata": { "name": "web" },
        "spec": { "replicas": "three" },
    }))
    .unwrap();
    let cells = typed_cells::<Deployment>(&malformed, |_| vec![Cell::Number(1)]);
    assert!(cells.is_empty());
}

/// The column table in the `resource-browser` spec, kind by kind: each listed
/// kind's own columns, titled and ordered as the spec (and `kubectl get`) has
/// them.
#[test]
fn every_kind_in_the_spec_has_its_columns() {
    let spec: &[(&str, &str, &[&str])] = &[
        ("apps", "Deployment", &["Ready", "Up-to-date", "Available"]),
        ("apps", "ReplicaSet", &["Desired", "Current", "Ready"]),
        ("apps", "StatefulSet", &["Ready"]),
        (
            "apps",
            "DaemonSet",
            &["Desired", "Current", "Ready", "Up-to-date", "Available"],
        ),
        ("batch", "Job", &["Status", "Completions", "Duration"]),
        (
            "batch",
            "CronJob",
            &["Schedule", "Suspend", "Active", "Last schedule"],
        ),
        ("", "ConfigMap", &["Data"]),
        ("", "Secret", &["Type", "Data"]),
        (
            "",
            "Service",
            &["Type", "Cluster IP", "External IP", "Ports"],
        ),
        (
            "networking.k8s.io",
            "Ingress",
            &["Class", "Hosts", "Address", "Ports"],
        ),
        ("", "Endpoints", &["Endpoints"]),
        (
            "discovery.k8s.io",
            "EndpointSlice",
            &["Address type", "Ports", "Endpoints"],
        ),
        ("networking.k8s.io", "NetworkPolicy", &["Pod selector"]),
        (
            "",
            "PersistentVolumeClaim",
            &[
                "Status",
                "Volume",
                "Capacity",
                "Access modes",
                "Storage class",
            ],
        ),
        (
            "",
            "PersistentVolume",
            &[
                "Capacity",
                "Access modes",
                "Reclaim policy",
                "Status",
                "Claim",
                "Storage class",
            ],
        ),
        (
            "storage.k8s.io",
            "StorageClass",
            &["Provisioner", "Reclaim policy", "Volume binding mode"],
        ),
        ("", "Node", &["Status", "Roles", "Version", "Internal IP"]),
        ("", "Namespace", &["Status"]),
        ("", "ServiceAccount", &["Secrets"]),
        (
            "rbac.authorization.k8s.io",
            "RoleBinding",
            &["Role", "Subjects"],
        ),
        (
            "rbac.authorization.k8s.io",
            "ClusterRoleBinding",
            &["Role", "Subjects"],
        ),
    ];
    for (group, kind, titles) in spec {
        let columns = super::for_kind(group, kind).unwrap_or_else(|| panic!("{kind} has columns"));
        let actual: Vec<&str> = columns.columns.iter().map(|column| column.title).collect();
        assert_eq!(&actual, titles, "{kind}'s columns");
        let mut ids: Vec<&str> = columns.columns.iter().map(|column| column.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), titles.len(), "{kind}'s column keys are distinct");
    }
}

/// The spec's "A kind without extra columns": a Roles list - and a CRD's -
/// shows only the base columns.
#[test]
fn a_kind_outside_the_table_has_no_columns_of_its_own() {
    assert!(super::for_kind("rbac.authorization.k8s.io", "Role").is_none());
    assert!(super::for_kind("example.com", "Widget").is_none());
    assert!(
        super::for_kind("", "Pod").is_none(),
        "Pods keep their own table"
    );
}
