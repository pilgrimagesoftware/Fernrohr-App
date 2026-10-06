//! `collapse-large-metadata-values` 1.3, in the object panel: a Deployment's
//! multi-line annotation shows only its preview with the full value on hover,
//! and a Secret's redacted last-applied-configuration annotation shows no
//! Secret value in its chip or a tooltip.

use super::fixtures::{kind, object, stub_panel, target};
use crate::k8s::resource::object_detail::redact::LAST_APPLIED_ANNOTATION;
use crate::ui::detail::{
    metadata_chip_id, metadata_chip_selector, metadata_copy_id, metadata_tooltip_selector,
};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use serde_json::{Value, json};
use std::time::Duration;

const ANNOTATIONS: &str = "Overview/Annotations";

fn loaded(
    cx: &mut TestAppContext,
    group: &str,
    kind_name: &str,
    manifest: Value,
) -> (WindowHandle<Root>, VisualTestContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let found = kind(group, "v1", kind_name, true);
    let (window, panel) = stub_panel(
        cx,
        target(found.clone(), Some("staging"), "web"),
        vec![found],
    );
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |_, _window, cx| {
            panel.update(cx, |panel, cx| panel.test_set_loaded(object(manifest), cx))
        })
        .unwrap();
    vcx.run_until_parked();
    (window, vcx)
}

fn chip(window: WindowHandle<Root>, vcx: &mut VisualTestContext, index: usize) -> Option<String> {
    vcx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(metadata_chip_id(ANNOTATIONS, index))
            .and_then(|chip| chip.value().map(str::to_string))
    })
    .unwrap()
}

fn hover(vcx: &mut VisualTestContext, index: usize) -> bool {
    let bounds = vcx
        .debug_bounds(metadata_chip_selector(ANNOTATIONS, index).leak())
        .expect("the chip is drawn");
    vcx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    vcx.debug_bounds(metadata_tooltip_selector(ANNOTATIONS, index).leak())
        .is_some()
}

/// Spec: "An object's long annotation is shortened".
#[gpui_kit::test]
async fn a_deployments_multi_line_annotation_is_shortened(cx: &mut TestAppContext) {
    let (window, mut vcx) = loaded(
        cx,
        "apps",
        "Deployment",
        json!({
            "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": {
                "name": "web", "namespace": "staging",
                "annotations": { "ad.example.com/checks": "{\n  \"nginx\": {}\n}" },
            },
        }),
    );
    assert_eq!(
        chip(window, &mut vcx, 0).as_deref(),
        Some("ad.example.com/checks={…")
    );
    assert!(hover(&mut vcx, 0), "hovering shows the full value");

    let copy = vcx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window
                .try_find(metadata_copy_id(ANNOTATIONS, 0))
                .expect("a shortened chip has a copy control")
                .bounds()
                .center()
        })
        .unwrap();
    vcx.simulate_click(copy, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(
        vcx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .as_deref(),
        Some("{\n  \"nginx\": {}\n}")
    );
}

/// Spec: "A redacted annotation stays redacted" - redaction leaves only a
/// short placeholder, so the chip shows that and nothing else, and has no
/// tooltip to carry more.
#[gpui_kit::test]
async fn a_secrets_redacted_annotation_stays_redacted(cx: &mut TestAppContext) {
    let applied = json!({
        "apiVersion": "v1", "kind": "Secret",
        "metadata": { "name": "web", "namespace": "staging" },
        "data": { "password": "c3VwZXItc2VjcmV0LXBhc3N3b3JkLXRoYXQtaXMtcXVpdGUtbG9uZy1pbmRlZWQ=" },
    })
    .to_string();
    let (window, mut vcx) = loaded(
        cx,
        "",
        "Secret",
        json!({
            "apiVersion": "v1", "kind": "Secret",
            "metadata": {
                "name": "web", "namespace": "staging",
                "annotations": { (LAST_APPLIED_ANNOTATION): applied },
            },
            "data": { "password": "c3VwZXItc2VjcmV0" },
        }),
    );
    let shown = chip(window, &mut vcx, 0).expect("the annotation is drawn");
    assert!(
        shown.starts_with(&format!("{LAST_APPLIED_ANNOTATION}=<redacted")),
        "only the placeholder: {shown}"
    );
    assert!(!shown.contains("c3VwZXI"), "no Secret value: {shown}");
    assert!(!hover(&mut vcx, 0), "no tooltip to reveal more");
    let has_copy = vcx
        .update_window(window.into(), |_, window, cx| {
            window.render_frame(cx);
            window.try_find(metadata_copy_id(ANNOTATIONS, 0)).is_some()
        })
        .unwrap();
    assert!(!has_copy, "no copy control to reveal more");
}
