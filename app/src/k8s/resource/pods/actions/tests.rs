//! Delete and Kill on a selected pod, in a real window with the app's keymap,
//! over a fake cluster that applies and records deletes: Kill goes at once with
//! no grace period, Delete asks first and Cancel sends nothing, and a refused
//! delete shows why while the row stays.

use super::{DISMISS_FAILURE_ID, FAILURE_ID};
use crate::k8s::resource::pods::test_window::{Harness, open};
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, TestAppContext};
use serde_json::json;

fn dialog_open(harness: &mut Harness) -> bool {
    harness
        .vcx
        .update(|window, cx| window.has_active_dialog(cx))
}

/// Presses the open dialog's `n`th button from the keyboard: Tab to it, then
/// Space - Cancel is the first, Delete the second.
fn press_dialog_button(harness: &mut Harness, n: usize) {
    for _ in 0..n {
        harness.vcx.simulate_keystrokes("tab");
        harness.vcx.run_until_parked();
    }
    let space = gpui_kit::Keystroke::parse("space").expect("valid");
    harness.vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    harness
        .vcx
        .simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    harness.vcx.run_until_parked();
}

fn listed(harness: &mut Harness) -> Vec<String> {
    harness.vcx.update(|_, cx| {
        harness
            .panel
            .read(cx)
            .table
            .read(cx)
            .pods()
            .iter()
            .filter_map(|pod| pod.metadata.name.clone())
            .collect()
    })
}

/// Spec "Force kill deletes immediately": no prompt, and a zero grace period.
#[gpui_kit::test]
async fn kill_deletes_the_pod_at_once_with_no_grace_period(cx: &mut TestAppContext) {
    let mut harness = open(cx);

    harness.press("ctrl-k");

    assert!(!dialog_open(&mut harness), "no confirmation");
    for _ in 0..400 {
        if !harness.cluster.deletes().is_empty() {
            break;
        }
        harness.vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let deletes = harness.cluster.deletes();
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].0, "web-1");
    assert_eq!(deletes[0].1["gracePeriodSeconds"], json!(0));
    harness.wait_for("dropped the row", |panel, cx| {
        panel.table.read(cx).pods().len() == 1
    });
    assert_eq!(listed(&mut harness), ["web-2"]);
}

/// Specs "Cancel leaves the resource untouched" and "Confirm deletes the
/// resource": Delete asks; Cancel sends nothing, and confirming deletes with
/// the pod's own grace period, its row going once the delete is observed.
#[gpui_kit::test]
async fn delete_asks_first_and_only_confirming_deletes(cx: &mut TestAppContext) {
    let mut harness = open(cx);

    harness.press("ctrl-d");
    assert!(dialog_open(&mut harness), "Delete asks first");
    press_dialog_button(&mut harness, 1);
    assert!(!dialog_open(&mut harness), "Cancel closes it");
    harness.vcx.run_until_parked();
    assert!(harness.cluster.deletes().is_empty(), "and sends nothing");
    assert_eq!(listed(&mut harness).len(), 2, "the row stays");

    harness.press("ctrl-d");
    press_dialog_button(&mut harness, 2);
    harness.wait_for("dropped the row", |panel, cx| {
        panel.table.read(cx).pods().len() == 1
    });
    let deletes = harness.cluster.deletes();
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].0, "web-1");
    assert_eq!(
        deletes[0].1.get("gracePeriodSeconds"),
        None,
        "the pod's own grace period"
    );
}

/// Spec "Delete request fails": a refused delete shows the server's reason,
/// and the row stays; Dismiss clears the banner.
#[gpui_kit::test]
async fn a_refused_delete_shows_why_and_keeps_the_row(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.cluster.refuse_deletes(
        "403 Forbidden",
        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
            "reason": "Forbidden", "code": 403,
            "message": "pods \"web-1\" is forbidden: User \"dev\" cannot delete resource \"pods\"" }),
    );

    harness.press("ctrl-k");
    harness.wait_for("showed the refusal", |panel, _| {
        panel.action_failure.is_some()
    });

    harness.vcx.update(|_, cx| {
        let failure = harness.panel.read(cx).action_failure.clone().unwrap();
        assert_eq!(failure.action, "Kill pod web-1");
        assert!(
            failure.failure.message.starts_with("Forbidden: "),
            "{failure:?}"
        );
    });
    assert_eq!(listed(&mut harness).len(), 2, "the row stays");
    harness.vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        harness.vcx.debug_bounds(FAILURE_ID).is_some(),
        "the banner is drawn"
    );

    let dismiss = harness.vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(DISMISS_FAILURE_ID).expect("drawn").bounds()
    });
    harness
        .vcx
        .simulate_click(dismiss.center(), Modifiers::none());
    harness.vcx.run_until_parked();
    assert!(
        harness
            .vcx
            .update(|_, cx| harness.panel.read(cx).action_failure.is_none()),
        "Dismiss clears it"
    );
}
