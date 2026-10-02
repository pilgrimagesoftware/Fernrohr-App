//! `resource-detail-ui-improvements` 5: the Managed Fields tab shows one
//! disclosure row per manager, collapsed by default, expanding only the row
//! the user opens - by keyboard (Tab to it, Space) and by mouse.

use super::config_fixture::{Harness, focus_panel, press_by_keyboard};
use super::states::{harness, starting_pod};
use crate::k8s::resource::pod_detail::managed_fields_view::{
    managed_field_body_selector, managed_field_key,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, ElementId, Modifiers, SharedString, TestAppContext, VisualTestContext,
};
use k8s_openapi::api::core::v1::Pod;
use serde_json::json;

/// [`starting_pod`] with two managers: kubectl's client-side apply, then the
/// kubelet's status update.
fn managed_pod() -> Pod {
    let mut pod = starting_pod();
    pod.metadata.managed_fields = Some(
        serde_json::from_value(json!([
            {
                "manager": "kubectl-client-side-apply", "operation": "Update",
                "time": "2026-10-02T10:00:00Z", "fieldsType": "FieldsV1",
                "fieldsV1": { "f:metadata": { "f:labels": { "f:app": {} } } },
            },
            {
                "manager": "kubelet", "operation": "Update", "subresource": "status",
                "time": "2026-10-02T10:01:00Z", "fieldsType": "FieldsV1",
                "fieldsV1": { "f:status": { "f:phase": {} } },
            },
        ]))
        .unwrap(),
    );
    pod
}

fn body_drawn(vcx: &mut VisualTestContext, h: &Harness, index: usize) -> bool {
    let _ = vcx.update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    let selector: &'static str = managed_field_body_selector(index).leak();
    vcx.debug_bounds(selector).is_some()
}

fn toggle_id(index: usize) -> ElementId {
    ElementId::Name(SharedString::from(managed_field_key(index)))
}

/// Opens the Managed Fields tab with its key, `6`.
fn open_tab(vcx: &mut VisualTestContext, h: &Harness) {
    focus_panel(vcx, h);
    vcx.simulate_keystrokes("6");
    vcx.run_until_parked();
}

/// Spec: "Expanding one manager", by keyboard - collapsed by default; Tab to
/// the kubectl row and Space expands it alone.
#[gpui_kit::test]
async fn one_manager_expands_by_keyboard_and_the_rest_stay_collapsed(cx: &mut TestAppContext) {
    let h = harness(cx, managed_pod());
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    open_tab(&mut vcx, &h);
    assert!(
        !body_drawn(&mut vcx, &h, 0) && !body_drawn(&mut vcx, &h, 1),
        "every manager starts collapsed"
    );

    press_by_keyboard(&mut vcx, &h, toggle_id(0));

    assert!(body_drawn(&mut vcx, &h, 0), "the kubectl row expanded");
    assert!(
        !body_drawn(&mut vcx, &h, 1),
        "the kubelet row stays collapsed"
    );
}

/// The mouse route: a click expands a row and a second click collapses it.
#[gpui_kit::test]
async fn a_manager_row_toggles_on_click(cx: &mut TestAppContext) {
    let h = harness(cx, managed_pod());
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    open_tab(&mut vcx, &h);
    let click = |vcx: &mut VisualTestContext| {
        let center = vcx
            .update_window(h.window.into(), |_, window, cx| {
                window.render_frame(cx);
                window
                    .try_find(toggle_id(1))
                    .expect("the kubelet row is drawn")
                    .bounds()
                    .center()
            })
            .unwrap();
        vcx.simulate_click(center, Modifiers::none());
        vcx.run_until_parked();
    };

    click(&mut vcx);
    assert!(body_drawn(&mut vcx, &h, 1));
    click(&mut vcx);
    assert!(!body_drawn(&mut vcx, &h, 1));
}
