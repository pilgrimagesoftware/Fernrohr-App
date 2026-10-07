//! #101: a kind's status field is drawn as a status - in its tone's colour,
//! its text kept - not as plain text.

use super::fixtures::{kind, object, stub_panel, target};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};
use serde_json::json;

#[gpui_kit::test]
async fn a_jobs_status_is_drawn_as_a_status(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let jobs = kind("batch", "v1", "Job", true);
    let (window, panel) = stub_panel(
        cx,
        target(jobs.clone(), Some("staging"), "migrate"),
        vec![jobs],
    );
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |_, _, cx| {
            panel.update(cx, |panel, cx| {
                panel.test_set_loaded(
                    object(json!({
                        "apiVersion": "batch/v1", "kind": "Job",
                        "metadata": { "name": "migrate", "namespace": "staging" },
                        "spec": { "template": {} },
                        "status": { "conditions": [{ "type": "Failed", "status": "True" }] },
                    })),
                    cx,
                )
            });
        })
        .unwrap();
    vcx.run_until_parked();
    let _ = vcx.update_window(window.into(), |_, window, cx| window.render_frame(cx));

    assert!(
        vcx.debug_bounds("object-status-Job/Status").is_some(),
        "the Job's status is drawn in its tone"
    );
}
