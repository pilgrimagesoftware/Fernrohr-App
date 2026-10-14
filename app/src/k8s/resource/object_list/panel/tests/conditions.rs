//! `resource-status-message` in a list panel: a kind outside the per-kind
//! table gains its Ready and Message columns once a loaded object reports
//! conditions (2.1, 2.5), and `d` on a row with a long message opens its
//! detail, where the full text is (5.2).

use super::{Harness, column_ids, focus_table, harness, object, press, record_opens, row_names};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_list::columns::Cell;
use crate::k8s::resource::pod_detail::DetailView;
use crate::k8s::resource::status_tone::ConditionStatus;
use crate::ui::style::Tone;
use gpui_kit::TestAppContext;
use kube::api::DynamicObject;
use kube::core::GroupVersionKind;
use kube_runtime::watcher;
use serde_json::json;

fn kustomizations() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("kustomize.toolkit.fluxcd.io", "v1", "Kustomization"),
        plural: "kustomizations".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// A Kustomization named `name` whose `status.conditions` are `conditions`.
fn kustomization(name: &str, conditions: serde_json::Value) -> DynamicObject {
    DynamicObject {
        data: json!({ "status": { "conditions": conditions } }),
        ..object(name, Some("flux-system"))
    }
}

fn ready(status: &str, message: &str) -> serde_json::Value {
    json!([{ "type": "Ready", "status": status, "message": message,
             "lastTransitionTime": "2026-10-01T00:00:00Z" }])
}

/// Delivers `object` as a watch `Apply` and lets the panel redraw.
fn deliver(h: &mut Harness, object: DynamicObject) {
    let objects = h.vcx.update(|_, cx| h.panel.read(cx).objects.clone());
    h.vcx.update(|_, cx| {
        objects.update(cx, |table, cx| {
            table.apply(watcher::Event::Apply(object));
            cx.notify();
        })
    });
    h.vcx.run_until_parked();
}

/// `name`'s row's cells, as the table holds them.
fn cells(h: &mut Harness, name: &str) -> Vec<Cell> {
    h.vcx.update(|_, cx| {
        let table = h.panel.read(cx).table.clone().expect("the table is drawn");
        let row = table
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .find(|row| row.object.name == name)
            .map(|row| row.object.cells.clone());
        row.expect("the row is listed")
    })
}

/// 2.1, 2.5: no conditions, no condition columns; a watch event bringing a
/// Ready condition adds Ready and Message, before Age, toned by its status.
#[gpui_kit::test]
async fn ready_and_message_columns_appear_once_an_object_reports_conditions(
    cx: &mut TestAppContext,
) {
    let mut h = harness(
        cx,
        kustomizations(),
        vec![object("pending", Some("flux-system"))],
    );
    assert_eq!(column_ids(&mut h), ["name", "namespace", "age"]);

    deliver(
        &mut h,
        kustomization("apps", ready("False", "kustomize build failed")),
    );
    deliver(
        &mut h,
        kustomization("infra", ready("True", "Applied revision: main@sha1:0123")),
    );
    assert_eq!(
        column_ids(&mut h),
        ["name", "namespace", "ready", "message", "age"]
    );
    assert_eq!(
        cells(&mut h, "apps"),
        [
            Cell::Condition(ConditionStatus::False),
            Cell::Status("kustomize build failed".into(), Tone::Bad),
        ]
    );
    assert_eq!(
        cells(&mut h, "infra")[0].tone(),
        Some(Tone::Good),
        "a True Ready reads Good"
    );
    assert_eq!(
        cells(&mut h, "pending"),
        [Cell::Empty, Cell::Empty],
        "an object without conditions shows empty cells"
    );
}

/// 2.5: conditions with no `Ready` among them bring Message but no Ready.
#[gpui_kit::test]
async fn conditions_without_ready_bring_message_only(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        kustomizations(),
        vec![kustomization(
            "apps",
            json!([{ "type": "Healthy", "status": "True", "message": "all checks pass" }]),
        )],
    );
    assert_eq!(column_ids(&mut h), ["name", "namespace", "message", "age"]);
    assert_eq!(
        cells(&mut h, "apps")[1],
        Cell::Status("all checks pass".into(), Tone::Neutral)
    );
}

/// 2.5: ascending by Ready puts False first, then Unknown, then True, and an
/// object without one before them all.
#[gpui_kit::test]
async fn sorting_by_ready_surfaces_failures_first(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        kustomizations(),
        vec![
            kustomization("healthy", ready("True", "ok")),
            kustomization("waiting", ready("Unknown", "reconciling")),
            kustomization("broken", ready("False", "failed")),
        ],
    );
    h.vcx.update(|_, cx| {
        let table = h.panel.read(cx).table.clone().expect("the table is drawn");
        table.update(cx, |table, _| table.delegate_mut().set_sort("ready", false));
    });
    assert_eq!(row_names(&mut h), ["broken", "waiting", "healthy"]);
}

/// 5.2: `d` on a row whose Message is too long for its cell opens that
/// object's detail panel, structured - where the full message is shown.
#[gpui_kit::test]
async fn d_opens_the_detail_of_a_row_with_a_long_message(cx: &mut TestAppContext) {
    let opened = record_opens(cx);
    let long = "Health check failed after 5m0s: timeout waiting for: \
                [Deployment/apps/web status: 'InProgress', Deployment/apps/api status: 'Failed']";
    let mut h = harness(
        cx,
        kustomizations(),
        vec![kustomization("apps", ready("False", long))],
    );
    focus_table(&mut h);
    press(&mut h.vcx, "down d");

    let opened = opened.borrow();
    assert_eq!(opened.len(), 1, "one open request");
    assert_eq!(opened[0].target.kind, kustomizations());
    assert_eq!(opened[0].target.name, "apps");
    assert_eq!(opened[0].view, Some(DetailView::Structured));
}
