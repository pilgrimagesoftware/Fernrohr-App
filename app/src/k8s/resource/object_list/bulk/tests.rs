//! Bulk actions from a kind's list, by real keystrokes: each action through its
//! prompt where it has one, its confirmation, the requests sent for every
//! checked object, and the outcome reported per object.

use super::super::test_harness::{Harness, kind, open};
use crate::ui::bulk::bar::BAR_SELECTOR;
use crate::ui::bulk::outcome::OUTCOME_SELECTOR;
use gpui_kit::TestAppContext;
use serde_json::json;

fn deployments(cx: &mut TestAppContext, names: &[&str]) -> Harness {
    open(cx, kind("apps", "Deployment", "deployments", true), names)
}

fn nodes(cx: &mut TestAppContext) -> Harness {
    open(cx, kind("", "Node", "nodes", false), &["node-a", "node-b"])
}

/// Tasks 2.1 and 2.2: a checked Deployment offers the rollout actions, through
/// their contexts, and a checked ConfigMap doesn't; nothing is offered, nor the
/// bar shown, with nothing checked.
#[gpui_kit::test]
async fn the_bar_and_palette_offer_only_what_the_kind_takes(cx: &mut TestAppContext) {
    let mut harness = deployments(cx, &["web", "api"]);
    assert!(!harness.in_context("BulkDelete"), "nothing checked yet");
    assert!(!harness.drawn(BAR_SELECTOR));
    harness.check_first(1);
    assert!(harness.drawn(BAR_SELECTOR));
    for context in ["BulkRestartRollout", "BulkScale", "BulkDelete", "BulkLabel"] {
        assert!(harness.in_context(context), "{context} for a Deployment");
    }
    assert!(!harness.in_context("BulkDrain"));
    assert!(!harness.in_context("BulkViewLogs"));
}

#[gpui_kit::test]
async fn a_configmap_offers_no_rollout_scale_or_node_action(cx: &mut TestAppContext) {
    let mut harness = open(cx, kind("", "ConfigMap", "configmaps", true), &["settings"]);
    harness.check_first(1);
    for context in [
        "BulkRestartRollout",
        "BulkScale",
        "BulkCordon",
        "BulkViewLogs",
    ] {
        assert!(!harness.in_context(context), "no {context} for a ConfigMap");
    }
    assert!(harness.in_context("BulkCopyYaml"));
}

/// Task 7.2: Restart Rollout asks, and Enter confirms - it is recoverable -
/// then each checked Deployment's template is stamped.
#[gpui_kit::test]
async fn restart_rollout_stamps_every_checked_deployment(cx: &mut TestAppContext) {
    let mut harness = deployments(cx, &["api", "web", "worker"]);
    harness.check_first(2);
    harness.press("f15");
    assert!(harness.dialog_open(), "it asks first");
    assert!(harness.cluster.patches().is_empty());
    harness.press("enter");

    let patches = harness.patches(2);
    let names: Vec<&str> = patches.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["api", "web"]);
    for (_, patch) in &patches {
        assert!(
            patch["spec"]["template"]["metadata"]["annotations"]
                ["kubectl.kubernetes.io/restartedAt"]
                .is_string()
        );
    }
    assert_eq!(harness.outcome(), "Restart Rollout: 2 succeeded");
    assert!(harness.drawn(OUTCOME_SELECTOR));
}

/// Tasks 3.2 and 7.3: Scale asks for a count - refusing one that isn't a whole
/// number, without going on - then confirms, then scales each checked
/// Deployment to it.
#[gpui_kit::test]
async fn scale_takes_a_count_then_scales_every_checked_deployment(cx: &mut TestAppContext) {
    let mut harness = deployments(cx, &["api", "web"]);
    harness.check_first(2);
    harness.press("f16");
    assert!(harness.dialog_open(), "the prompt is open");
    harness.vcx.simulate_input("three");
    harness.press("enter");
    assert!(
        harness.drawn("bulk-scale-error"),
        "a count that isn't a number is refused"
    );
    assert!(harness.cluster.patches().is_empty());

    harness.press("secondary-a backspace");
    harness.vcx.simulate_input("3");
    harness.press("enter");
    assert!(harness.dialog_open(), "the confirmation follows");
    assert!(harness.cluster.patches().is_empty(), "nothing sent yet");
    harness.press("enter");

    let patches = harness.patches(2);
    for (_, patch) in &patches {
        assert_eq!(patch, &json!({ "spec": { "replicas": 3 } }));
    }
    assert_eq!(harness.outcome(), "Scale: 2 succeeded");
}

/// Task 7.4: Cordon marks every checked Node unschedulable.
#[gpui_kit::test]
async fn cordon_marks_every_checked_node_unschedulable(cx: &mut TestAppContext) {
    let mut harness = nodes(cx);
    harness.check_first(2);
    harness.press("f17");
    harness.press("enter");
    let patches = harness.patches(2);
    let names: Vec<&str> = patches.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["node-a", "node-b"]);
    assert!(
        patches
            .iter()
            .all(|(_, patch)| patch == &json!({ "spec": { "unschedulable": true } }))
    );
}

/// Drain is irreversible: Enter cancels, and only the deliberate shortcut
/// drains - cordoning each checked Node first.
#[gpui_kit::test]
async fn drain_confirms_only_by_the_deliberate_shortcut(cx: &mut TestAppContext) {
    let mut harness = nodes(cx);
    harness.check_first(2);
    harness.press("f18");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("enter");
    assert!(!harness.dialog_open());
    assert!(harness.cluster.patches().is_empty(), "Enter cancelled");

    harness.press("f18");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("secondary-backspace");
    let patches = harness.patches(2);
    assert_eq!(patches.len(), 2, "both cordoned");
    assert_eq!(harness.outcome(), "Drain: 2 succeeded");
}

/// Tasks 3.3 and the outcome requirement: a refused batch names each object
/// that failed, and why.
#[gpui_kit::test]
async fn a_refused_batch_names_each_failure(cx: &mut TestAppContext) {
    let mut harness = nodes(cx);
    harness.cluster.refuse_patches(
        "403 Forbidden",
        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
            "reason": "Forbidden", "code": 403, "message": "nodes is forbidden" }),
    );
    harness.check_first(2);
    harness.press("f17");
    harness.press("enter");
    assert_eq!(harness.outcome(), "Cordon: 0 succeeded, 2 failed");
    let failed = harness.vcx.update(|_, cx| {
        harness
            .panel
            .read(cx)
            .bulk_outcome
            .clone()
            .expect("shown")
            .failed
    });
    let names: Vec<&str> = failed.iter().map(|failed| failed.name.as_str()).collect();
    assert_eq!(names, ["node-a", "node-b"]);
}

/// Task 7.5: Label asks for a key and a value, confirms, then labels every
/// checked object.
#[gpui_kit::test]
async fn label_sets_the_key_on_every_checked_object(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        kind("", "ConfigMap", "configmaps", true),
        &["flags", "settings"],
    );
    harness.check_first(2);
    harness.press("f19");
    harness.vcx.simulate_input("team");
    harness.press("tab");
    harness.vcx.simulate_input("platform");
    harness.press("enter");
    assert!(harness.dialog_open(), "the confirmation follows");
    harness.press("enter");
    let patches = harness.patches(2);
    for (_, patch) in &patches {
        assert_eq!(
            patch,
            &json!({ "metadata": { "labels": { "team": "platform" } } })
        );
    }
}

/// Task 6.2: Copy YAML copies each checked object's manifest, `---` between,
/// with no confirmation.
#[gpui_kit::test]
async fn copy_yaml_copies_every_manifest_without_asking(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        kind("", "ConfigMap", "configmaps", true),
        &["flags", "settings"],
    );
    harness.check_first(2);
    harness.press("f20");
    assert!(!harness.dialog_open(), "no confirmation");
    harness.wait_for("copied", |harness| {
        harness
            .vcx
            .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
            .is_some()
    });
    let copied = harness
        .vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .unwrap_or_default();
    assert_eq!(copied.matches("---\n").count(), 1, "{copied}");
    assert!(copied.contains("name: flags") && copied.contains("name: settings"));
}

fn clipboard(harness: &mut Harness) -> Option<String> {
    harness
        .vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn set_clipboard(harness: &mut Harness, text: &str) {
    harness.vcx.update(|_, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text.to_string()))
    });
}

/// Review fix 5: with no manifest read, Copy YAML leaves the clipboard as it
/// was and reports each object it couldn't read.
#[gpui_kit::test]
async fn copy_yaml_with_nothing_read_leaves_the_clipboard_alone(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        kind("", "ConfigMap", "configmaps", true),
        &["flags", "settings"],
    );
    harness.check_first(2);
    set_clipboard(&mut harness, "before");
    harness.cluster.refuse_gets(
        "403 Forbidden",
        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
            "reason": "Forbidden", "code": 403, "message": "configmaps is forbidden" }),
    );
    harness.press("f20");
    assert_eq!(harness.outcome(), "Copy YAML: 0 succeeded, 2 failed");
    assert_eq!(clipboard(&mut harness).as_deref(), Some("before"));
}

/// Review fix 5: Copy YAML when the context isn't connected says so, rather
/// than doing nothing.
#[gpui_kit::test]
async fn copy_yaml_while_disconnected_says_so(cx: &mut TestAppContext) {
    use crate::k8s::cluster::connection::ConnectionState;
    let mut harness = open(
        cx,
        kind("", "ConfigMap", "configmaps", true),
        &["flags", "settings"],
    );
    harness.check_first(2);
    set_clipboard(&mut harness, "before");
    let connection = harness
        .vcx
        .update(|_, cx| harness.panel.read(cx).connection.clone());
    harness.vcx.update(|_, cx| {
        connection.update(cx, |connection, cx| {
            connection.state = ConnectionState::Connecting;
            cx.notify();
        })
    });
    // Not connected, the panel draws no table to take keys: this is a batch
    // asked for just as the connection drops, so the command is dispatched to
    // the panel itself.
    let panel = harness.panel.clone();
    harness.vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        let focus = panel.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    harness
        .vcx
        .dispatch_action(crate::ui::bulk::commands::BulkCopyYaml);
    harness.vcx.run_until_parked();
    assert_eq!(harness.outcome(), "Copy YAML: 0 succeeded, 2 failed");
    let failed = harness.vcx.update(|_, cx| {
        harness
            .panel
            .read(cx)
            .bulk_outcome
            .clone()
            .expect("shown")
            .failed
    });
    assert!(failed[0].failure.message.ends_with("is not connected."));
    assert_eq!(clipboard(&mut harness).as_deref(), Some("before"));
}

/// Review fix 6: a key the API server would refuse is refused in the prompt,
/// once, and nothing is sent.
#[gpui_kit::test]
async fn label_refuses_an_invalid_key_in_the_prompt(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        kind("", "ConfigMap", "configmaps", true),
        &["flags", "settings"],
    );
    harness.check_first(2);
    harness.press("f19");
    harness.vcx.simulate_input("Example.com/team");
    harness.press("tab");
    harness.vcx.simulate_input("platform");
    harness.press("enter");
    assert!(harness.drawn("bulk-metadata-error"), "the prompt says why");
    assert!(harness.dialog_open(), "and stays open");
    assert!(harness.cluster.patches().is_empty());
}

/// The names of the checked objects, top to bottom.
fn checked(harness: &mut Harness) -> Vec<String> {
    use crate::ui::bulk::BulkHost as _;
    let panel = harness.panel.clone();
    harness.vcx.update(|_, cx| {
        panel
            .read(cx)
            .bulk_selection(cx)
            .map(|selection| selection.targets.into_iter().map(|t| t.name).collect())
            .unwrap_or_default()
    })
}

/// #308: Cmd/Ctrl+`a` checks every row of any kind's list.
#[gpui_kit::test]
async fn select_all_checks_every_row(cx: &mut TestAppContext) {
    let mut harness = deployments(cx, &["api", "web", "worker"]);
    harness.press("secondary-a");
    assert_eq!(checked(&mut harness), ["api", "web", "worker"]);
    assert!(harness.drawn(BAR_SELECTOR));
}

/// The mouse route: a row's context menu ends with Check All Rows.
#[gpui_kit::test]
async fn the_row_menus_check_all_rows_checks_every_row(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent};
    let mut harness = deployments(cx, &["api", "web", "worker"]);
    harness.vcx.update(|window, cx| window.render_frame(cx));
    let row = harness
        .vcx
        .debug_bounds("object-cell-1-0")
        .expect("the row is drawn")
        .center();
    harness.vcx.simulate_event(MouseDownEvent {
        position: row,
        modifiers: Modifiers::none(),
        button: MouseButton::Right,
        click_count: 1,
        first_mouse: false,
    });
    harness.vcx.simulate_event(MouseUpEvent {
        position: row,
        modifiers: Modifiers::none(),
        button: MouseButton::Right,
        click_count: 1,
    });
    harness.vcx.run_until_parked();
    // Check All Rows is last: Up wraps to it.
    harness.press("up");
    harness.press("enter");
    assert_eq!(checked(&mut harness), ["api", "web", "worker"]);
    assert!(harness.drawn(BAR_SELECTOR), "the bar shows");
}
