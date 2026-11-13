//! #352: a CRD's printer columns, read with nothing known about the CRD.

use super::super::Cell;
use super::PrinterColumns;
use crate::k8s::resource::status_tone::ConditionStatus;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::api::DynamicObject;
use serde_json::json;

/// A CRD serving `v1` with `columns`, and `v2` with none.
fn crd(columns: serde_json::Value) -> CustomResourceDefinition {
    serde_json::from_value(json!({
        "apiVersion": "apiextensions.k8s.io/v1",
        "kind": "CustomResourceDefinition",
        "metadata": { "name": "widgets.example.com" },
        "spec": {
            "group": "example.com",
            "scope": "Namespaced",
            "names": { "plural": "widgets", "kind": "Widget" },
            "versions": [
                { "name": "v1", "served": true, "storage": true,
                  "additionalPrinterColumns": columns },
                { "name": "v2", "served": true, "storage": false },
            ],
        },
    }))
    .expect("a valid CRD")
}

fn widget(data: serde_json::Value) -> DynamicObject {
    let mut object = json!({
        "apiVersion": "example.com/v1",
        "kind": "Widget",
        "metadata": { "name": "w", "creationTimestamp": "2026-11-01T00:00:00Z" },
    });
    object
        .as_object_mut()
        .unwrap()
        .extend(data.as_object().unwrap().clone());
    serde_json::from_value(object).expect("a valid object")
}

fn titles(printer: &PrinterColumns) -> Vec<&str> {
    printer.columns().iter().map(|c| c.title.as_str()).collect()
}

/// Priority-0 columns in the CRD's order; `-o wide` ones and repeats of the
/// base Name, Namespace and Age columns are left out.
#[test]
fn columns_are_what_kubectl_get_shows() {
    let printer = PrinterColumns::from_crd(
        &crd(json!([
            { "name": "Ready", "type": "string", "jsonPath": ".status.ready" },
            { "name": "Detail", "type": "string", "jsonPath": ".status.detail", "priority": 1 },
            { "name": "Age", "type": "date", "jsonPath": ".metadata.creationTimestamp" },
            { "name": "Size", "type": "integer", "jsonPath": ".spec.size" },
        ])),
        "v1",
    )
    .expect("columns");
    assert_eq!(titles(&printer), ["Ready", "Size"]);
    assert_eq!(printer.columns()[0].id, "crd:ready");
}

/// No columns left, or a version the CRD doesn't serve them for, is `None`:
/// the list keeps its generic columns.
#[test]
fn no_columns_is_none() {
    let only_age = crd(json!([
        { "name": "Age", "type": "date", "jsonPath": ".metadata.creationTimestamp" },
    ]));
    assert_eq!(PrinterColumns::from_crd(&only_age, "v1"), None);
    let sized = crd(json!([{ "name": "Size", "type": "integer", "jsonPath": ".spec.size" }]));
    assert_eq!(PrinterColumns::from_crd(&sized, "v2"), None, "v2 has none");
    assert_eq!(PrinterColumns::from_crd(&sized, "v9"), None, "not served");
}

/// Each declared type reads as its cell: integers sort as numbers, dates as
/// ages, a condition status in its tone, kubectl's filter syntax works, and
/// a path matching several values shows the first, as the API server does.
#[test]
fn cells_follow_the_declared_type() {
    let printer = PrinterColumns::from_crd(
        &crd(json!([
            { "name": "Ready", "type": "string",
              "jsonPath": ".status.conditions[?(@.type==\"Ready\")].status" },
            { "name": "Size", "type": "integer", "jsonPath": ".spec.size" },
            { "name": "Synced", "type": "date", "jsonPath": ".status.lastSync" },
            { "name": "Mode", "type": "string", "jsonPath": ".spec.mode" },
            { "name": "Hosts", "type": "string", "jsonPath": ".spec.hosts[*]" },
            { "name": "Paused", "type": "boolean", "jsonPath": ".spec.paused" },
            { "name": "Missing", "type": "string", "jsonPath": ".status.nothing" },
        ])),
        "v1",
    )
    .expect("columns");
    let cells = printer.cells_for(&widget(json!({
        "spec": { "size": 3, "mode": "fast", "hosts": ["a", "b"], "paused": false },
        "status": {
            "lastSync": "2026-11-02T00:00:00Z",
            "conditions": [
                { "type": "Synced", "status": "False" },
                { "type": "Ready", "status": "True" },
            ],
        },
    })));
    assert_eq!(
        cells,
        vec![
            Cell::Condition(ConditionStatus::True),
            Cell::Number(3),
            Cell::Age("2026-11-02T00:00:00Z".parse().unwrap()),
            Cell::Text("fast".into()),
            Cell::Text("a".into()),
            Cell::Text("false".into()),
            Cell::Empty,
        ]
    );
}

/// A `jsonPath` that doesn't parse drops its column rather than leaving it
/// empty on every row.
#[test]
fn an_unparseable_path_drops_its_column() {
    let printer = PrinterColumns::from_crd(
        &crd(json!([
            { "name": "Broken", "type": "string", "jsonPath": ".spec[?(" },
            { "name": "Size", "type": "integer", "jsonPath": ".spec.size" },
        ])),
        "v1",
    )
    .expect("columns");
    assert_eq!(titles(&printer), ["Size"]);
}

/// An object missing everything gets one empty cell per column - nothing
/// misaligns.
#[test]
fn an_object_without_the_fields_gets_empty_cells() {
    let printer = PrinterColumns::from_crd(
        &crd(json!([
            { "name": "Size", "type": "integer", "jsonPath": ".spec.size" },
            { "name": "Phase", "type": "string", "jsonPath": "status.phase" },
        ])),
        "v1",
    )
    .expect("columns");
    assert_eq!(
        printer.cells_for(&widget(json!({}))),
        vec![Cell::Empty, Cell::Empty]
    );
}

/// A store given printer columns builds its rows with their cells, and the
/// list lays them out between Namespace and Age in place of the condition
/// columns.
#[test]
fn a_list_shows_its_printer_columns() {
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use crate::k8s::resource::object_list::store::ObjectsTable;
    use crate::k8s::resource::object_list::table::ListColumn;
    use kube::api::GroupVersionKind;

    let kind = DiscoveredKind {
        gvk: GroupVersionKind::gvk("example.com", "v1", "Widget"),
        plural: "widgets".into(),
        namespaced: true,
        verbs: Default::default(),
    };
    let printer = PrinterColumns::from_crd(
        &crd(json!([{ "name": "Size", "type": "integer", "jsonPath": ".spec.size" }])),
        "v1",
    )
    .expect("columns");
    let mut table = ObjectsTable::for_kind(&kind);
    table.set_printer_columns(printer);
    let mut object = widget(json!({ "spec": { "size": 7 } }));
    object.metadata.uid = Some("u1".into());
    table.replace_all(vec![object]);
    assert_eq!(table.rows()[0].cells, vec![Cell::Number(7)]);

    let every_condition = super::super::ConditionsSeen {
        conditions: true,
        ready: true,
        transition: true,
    };
    let columns = ListColumn::for_kind(&kind, every_condition, table.printer_columns());
    let ids: Vec<&str> = columns.iter().map(|column| column.id.as_ref()).collect();
    assert_eq!(ids, ["name", "namespace", "crd:size", "age"]);
}
