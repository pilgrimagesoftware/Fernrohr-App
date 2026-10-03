//! `collapse-large-metadata-values` 1.3, in the pod panel: a multi-line or
//! long annotation's chip shows only its preview, hovering it shows the full
//! value, Tab and Space on its copy control put the full value on the
//! clipboard, and a short label is drawn whole with neither.

use super::config_fixture::{Harness, focus_panel, press_by_keyboard};
use super::states::harness;
use crate::ui::detail::{
    metadata_chip_id, metadata_chip_selector, metadata_copy_id, metadata_tooltip_selector,
};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Modifiers, TestAppContext, VisualTestContext};
use k8s_openapi::api::core::v1::Pod;
use serde_json::json;
use std::time::Duration;

const CHECKS: &str = "{\n  \"nginx\": {\n    \"init_config\": {}\n  }\n}";

fn annotated_pod() -> Pod {
    serde_json::from_value(json!({
        "metadata": {
            "name": "api-7d9f-ftg5t", "namespace": "staging", "uid": "u1",
            "labels": { "app": "api" },
            "annotations": {
                "ad.example.com/checks": CHECKS,
                "sidecar.example.com/status": "s".repeat(300),
            },
        },
        "spec": { "containers": [{ "name": "app", "image": "registry.example/api:1.2.3" }] },
        "status": { "phase": "Running" },
    }))
    .unwrap()
}

/// What chip `index` of the `field` row shows.
fn chip(vcx: &mut VisualTestContext, h: &Harness, field: &str, index: usize) -> Option<String> {
    vcx.update_window(h.window.into(), |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(metadata_chip_id(field, index))
            .and_then(|chip| chip.value().map(str::to_string))
    })
    .unwrap()
}

/// Hovers chip `index` of `field` past the tooltip delay; whether its tooltip
/// is drawn.
fn hover(vcx: &mut VisualTestContext, field: &str, index: usize) -> bool {
    let bounds = vcx
        .debug_bounds(metadata_chip_selector(field, index).leak())
        .expect("the chip is drawn");
    vcx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    vcx.debug_bounds(metadata_tooltip_selector(field, index).leak())
        .is_some()
}

#[gpui_kit::test]
async fn large_annotations_show_a_preview_and_the_full_value_on_hover(cx: &mut TestAppContext) {
    let h = harness(cx, annotated_pod());
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    vcx.run_until_parked();

    assert_eq!(
        chip(&mut vcx, &h, "Annotations", 0).as_deref(),
        Some("ad.example.com/checks={…"),
        "a multi-line value shows its first line's start"
    );
    assert_eq!(
        chip(&mut vcx, &h, "Annotations", 1),
        Some(format!("sidecar.example.com/status={}…", "s".repeat(20))),
        "a long value shows its first 20 characters"
    );
    assert_eq!(
        chip(&mut vcx, &h, "Labels", 0).as_deref(),
        Some("app=api"),
        "a short value is whole"
    );

    assert!(!hover(&mut vcx, "Labels", 0), "a short chip has no tooltip");
    assert!(
        hover(&mut vcx, "Annotations", 1),
        "a shortened chip has one"
    );
}

/// The keyboard route to a shortened value: its copy control is a tab stop,
/// and Space copies the full value. A short chip has no copy control.
#[gpui_kit::test]
async fn a_shortened_value_copies_in_full_by_keyboard(cx: &mut TestAppContext) {
    let h = harness(cx, annotated_pod());
    let mut vcx = VisualTestContext::from_window(h.window.into(), cx);
    vcx.run_until_parked();
    focus_panel(&mut vcx, &h);

    press_by_keyboard(&mut vcx, &h, metadata_copy_id("Annotations", 0));
    assert_eq!(
        vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .as_deref(),
        Some(CHECKS)
    );
    let short_has_copy = vcx
        .update_window(h.window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.try_find(metadata_copy_id("Labels", 0)).is_some()
        })
        .unwrap();
    assert!(!short_has_copy, "a short chip has no copy control");
}
