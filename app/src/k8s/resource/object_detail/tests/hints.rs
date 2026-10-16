//! The object viewer's header hint row (`key-hint-rows` 2.1): a narrow
//! window collapses it to the view toggle plus the overflow control, a wide
//! one shows every hint.

use super::fixtures::{kind, object, stub_panel, target};
use crate::k8s::resource::object_detail::render::HEADER_HINTS_ID;
use crate::ui::key_hint_row::{control_selector, row_selector};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext, px, size};

fn deployment_kind() -> crate::k8s::cluster::discovery::DiscoveredKind {
    kind("apps", "v1", "Deployment", true)
}

/// A Deployment with an owner reference to a Pod (the one core kind
/// "Go to..." always resolves): the view toggle, Logs, Pause
/// rollout, Delete and Go to... - more hints than a narrow header holds in three lines.
fn deployment() -> kube::api::DynamicObject {
    object(serde_json::json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {
            "name": "web",
            "namespace": "staging",
            "uid": "deploy-1",
            "creationTimestamp": "1970-01-01T00:00:00Z",
            "ownerReferences": [{
                "apiVersion": "v1",
                "kind": "Pod",
                "name": "web-0",
                "uid": "pod-1",
                "controller": true,
            }],
        },
        "spec": { "replicas": 2 },
        "status": {},
    }))
}

fn drawn(vcx: &mut VisualTestContext, selector: String) -> bool {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.debug_bounds(selector.leak()).is_some()
}

fn resized(cx: &mut TestAppContext, width: f32) -> VisualTestContext {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (window, panel) = stub_panel(
        cx,
        target(deployment_kind(), Some("staging"), "web"),
        vec![deployment_kind(), kind("", "v1", "Pod", true)],
    );
    window
        .update(cx, |_, _, cx| {
            panel.update(cx, |panel, cx| panel.test_set_loaded(deployment(), cx))
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_resize(size(px(width), px(700.)));
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
    }
    vcx
}

#[gpui_kit::test]
async fn a_narrow_object_header_never_wraps_past_three_lines(cx: &mut TestAppContext) {
    // The object header's hints are few (at most Go to, the kind's actions,
    // Logs, Delete and the toggle) and its row does not narrow below its
    // widest hints, so even a Deployment with every hint fits three lines
    // and stays expanded: the collapse is only a guard here. Either way the
    // header must stay within three lines.
    let mut vcx = resized(cx, 260.);
    let collapsed = drawn(&mut vcx, row_selector(HEADER_HINTS_ID, true));
    let expanded = drawn(&mut vcx, row_selector(HEADER_HINTS_ID, false));
    assert!(collapsed ^ expanded, "exactly one row state is drawn");
    let row = vcx
        .debug_bounds(row_selector(HEADER_HINTS_ID, collapsed).leak())
        .expect("the row");
    assert!(
        row.size.height < px(80.),
        "within three 22.5px lines: {:?}",
        row.size
    );
    assert_eq!(
        drawn(&mut vcx, control_selector(HEADER_HINTS_ID)),
        collapsed,
        "the overflow control is drawn exactly while collapsed"
    );
}

#[gpui_kit::test]
async fn a_wide_object_header_shows_every_hint(cx: &mut TestAppContext) {
    let mut vcx = resized(cx, 2400.);
    assert!(drawn(&mut vcx, row_selector(HEADER_HINTS_ID, false)));
    assert!(!drawn(&mut vcx, control_selector(HEADER_HINTS_ID)));
}
