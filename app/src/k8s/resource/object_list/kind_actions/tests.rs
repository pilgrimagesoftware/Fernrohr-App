//! Kind-specific actions from a kind's list, by their default keys: each
//! through its prompt and confirmation where it has one, the request it makes,
//! and what the row is offered after. Over a fake cluster that watches, stores
//! creates and merges patches.

use super::super::test_harness::{Harness, kind, open, open_objects};
use crate::k8s::test_cluster::TEST_TOKEN;
use gpui_kit::TestAppContext;
use serde_json::{Value, json};

fn meta(name: &str) -> Value {
    json!({ "name": name, "namespace": "shop", "uid": format!("uid-{name}") })
}

fn cronjob(suspend: bool) -> Value {
    json!({ "apiVersion": "batch/v1", "kind": "CronJob", "metadata": meta("backup"),
        "spec": { "schedule": "0 3 * * *", "suspend": suspend, "jobTemplate": {
            "metadata": { "labels": { "app": "backup" } },
            "spec": { "template": { "spec": { "restartPolicy": "Never",
                "containers": [{ "name": "backup", "image": "backup:1" }] } } } } } })
}

fn cronjobs(cx: &mut TestAppContext, suspend: bool) -> Harness {
    open_objects(
        cx,
        kind("batch", "CronJob", "cronjobs", true),
        vec![cronjob(suspend)],
    )
}

/// Task 2.2 and the keyboard requirement: `t` on a CronJob row asks, and
/// confirming creates a Job from its template - the CronJob itself unwritten.
#[gpui_kit::test]
async fn t_triggers_a_cronjob_after_asking(cx: &mut TestAppContext) {
    let mut harness = cronjobs(cx, false);
    assert!(harness.in_context("KindTriggerNow"));
    harness.press("t");
    assert!(harness.dialog_open(), "it asks first");
    assert!(harness.cluster.creates().is_empty());
    harness.press("enter");
    harness.wait_for("created the Job", |harness| {
        !harness.cluster.creates().is_empty()
    });
    let (plural, job) = harness.cluster.creates()[0].clone();
    assert_eq!(plural, "jobs");
    assert!(
        job["metadata"]["name"]
            .as_str()
            .unwrap()
            .starts_with("backup-manual-")
    );
    assert_eq!(
        job["spec"]["template"]["spec"]["containers"][0]["image"],
        json!("backup:1")
    );
    assert!(
        harness.cluster.patches().is_empty(),
        "the CronJob is unchanged"
    );
}

/// Task 2.3: Suspend on an active CronJob sets `spec.suspend` - only on the
/// listed object - and the row is offered Resume after.
#[gpui_kit::test]
async fn suspend_flips_the_row_to_resume(cx: &mut TestAppContext) {
    let mut harness = cronjobs(cx, false);
    assert!(harness.in_context("KindSuspend") && !harness.in_context("KindResume"));
    harness.press("p");
    harness.press("enter");
    let patches = harness.patches(1);
    assert_eq!(
        patches[0].1,
        json!({ "metadata": { "uid": "uid-backup" }, "spec": { "suspend": true } })
    );
    harness.wait_for("offered Resume", |harness| harness.in_context("KindResume"));
    assert!(!harness.in_context("KindSuspend"));
}

/// Task 3.2: a Deployment is offered Pause Rollout; a DaemonSet or StatefulSet
/// list nothing of the kind.
#[gpui_kit::test]
async fn pause_rollout_is_a_deployments_alone(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        kind("apps", "Deployment", "deployments", true),
        &["web"],
    );
    assert!(harness.in_context("KindPauseRollout"));
    harness.press("p");
    harness.press("enter");
    assert_eq!(
        harness.patches(1)[0].1,
        json!({ "metadata": { "uid": "uid-web" }, "spec": { "paused": true } })
    );
    harness.wait_for("offered Resume Rollout", |harness| {
        harness.in_context("KindResumeRollout")
    });
}

#[gpui_kit::test]
async fn a_daemonset_is_offered_no_pause(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        kind("apps", "DaemonSet", "daemonsets", true),
        &["agent"],
    );
    assert!(!harness.in_context("KindPauseRollout"));
    harness.press("p");
    assert!(!harness.dialog_open(), "p does nothing on a DaemonSet");
}

fn claim() -> Value {
    json!({ "apiVersion": "v1", "kind": "PersistentVolumeClaim", "metadata": meta("data"),
        "spec": { "resources": { "requests": { "storage": "10Gi" } } } })
}

fn claims(cx: &mut TestAppContext) -> Harness {
    open_objects(
        cx,
        kind("", "PersistentVolumeClaim", "persistentvolumeclaims", true),
        vec![claim()],
    )
}

/// Task 4.2: Expand refuses a size no larger than the claim's, in the prompt,
/// then confirms - irreversibly - and patches the new size.
#[gpui_kit::test]
async fn expand_refuses_a_smaller_size_then_patches_a_larger_one(cx: &mut TestAppContext) {
    let mut harness = claims(cx);
    harness.press("shift-e");
    harness.vcx.simulate_input("5Gi");
    harness.press("enter");
    assert!(
        harness.drawn("kind-expand-error"),
        "a smaller size is refused"
    );
    harness.press("secondary-a backspace");
    harness.vcx.simulate_input("20Gi");
    harness.press("enter");
    assert!(harness.dialog_open(), "the confirmation follows");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("enter");
    assert!(
        harness.cluster.patches().is_empty(),
        "Enter is Cancel's: it can't be undone"
    );
    harness.press("shift-e");
    harness.vcx.simulate_input("20Gi");
    harness.press("enter");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("secondary-backspace");
    assert_eq!(
        harness.patches(1)[0].1["spec"]["resources"]["requests"]["storage"],
        json!("20Gi")
    );
}

/// Task 4.1/4.2: the StorageClass's refusal is shown as the cluster gave it.
#[gpui_kit::test]
async fn a_refused_expansion_shows_the_clusters_words(cx: &mut TestAppContext) {
    let mut harness = claims(cx);
    harness.cluster.refuse_patches(
        "403 Forbidden",
        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure", "reason": "Forbidden",
            "code": 403, "message": "only dynamically provisioned pvc can be resized and the \
            storageclass that provisions the pvc must support resize" }),
    );
    harness.press("shift-e");
    harness.vcx.simulate_input("20Gi");
    harness.press("enter");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("secondary-backspace");
    harness.wait_for("showed the refusal", |harness| {
        harness
            .vcx
            .update(|_, cx| harness.panel.read(cx).refusal.is_some())
    });
    let refusal = harness
        .vcx
        .update(|_, cx| harness.panel.read(cx).refusal.clone())
        .expect("shown");
    let said = format!("{} {}", refusal.failure.message, refusal.failure.detail);
    assert!(said.contains("must support resize"), "{said}");
}

fn node(taints: Value) -> Value {
    json!({ "apiVersion": "v1", "kind": "Node",
        "metadata": { "name": "node-a", "uid": "uid-node-a" }, "spec": { "taints": taints } })
}

/// Task 5.2: Add Taint by keyboard - key, value, effect, confirm.
#[gpui_kit::test]
async fn add_taint_by_keyboard(cx: &mut TestAppContext) {
    let mut harness = open_objects(cx, kind("", "Node", "nodes", false), vec![node(json!([]))]);
    harness.press("shift-t");
    harness.vcx.simulate_input("dedicated");
    harness.press("tab");
    harness.vcx.simulate_input("gpu");
    harness.press("tab");
    harness.vcx.simulate_input("NoSchedule");
    harness.press("enter");
    assert!(harness.dialog_open(), "the confirmation follows");
    harness.press("enter");
    let patch = harness.patches(1)[0].1.clone();
    assert_eq!(
        patch["spec"]["taints"],
        json!([{ "key": "dedicated", "value": "gpu", "effect": "NoSchedule" }])
    );
    assert_eq!(patch["metadata"]["uid"], json!("uid-node-a"));
}

/// Task 5.2: Remove Taint asks which of two, and removes only that one.
#[gpui_kit::test]
async fn remove_taint_by_keyboard_removes_only_the_chosen_one(cx: &mut TestAppContext) {
    let taints = json!([
        { "key": "dedicated", "value": "gpu", "effect": "NoSchedule" },
        { "key": "team", "effect": "PreferNoSchedule" },
    ]);
    let mut harness = open_objects(cx, kind("", "Node", "nodes", false), vec![node(taints)]);
    harness.press("alt-shift-t");
    assert!(harness.dialog_open(), "it asks which");
    // Tab reaches the first taint's button, Space presses it.
    harness.press("tab");
    harness.press_space();
    assert!(harness.dialog_open(), "then confirms");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("enter");
    let patch = harness.patches(1)[0].1.clone();
    assert_eq!(
        patch["spec"]["taints"],
        json!([{ "key": "team", "effect": "PreferNoSchedule" }]),
        "only the first is gone"
    );
}

/// Task 6.2: Create Token shows the token once; Hide drops it - and the panel
/// never holds it.
#[gpui_kit::test]
async fn create_token_shows_it_once_and_hide_drops_it(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        kind("", "ServiceAccount", "serviceaccounts", true),
        &["deploy"],
    );
    harness.press("shift-k");
    harness.wait_for("showed the token", |harness| harness.dialog_open());
    let (_, request) = harness.cluster.creates()[0].clone();
    assert_eq!(
        request,
        json!({ "apiVersion": "authentication.k8s.io/v1", "kind": "TokenRequest", "spec": {} })
    );
    assert!(harness.drawn(crate::ui::kind_actions::token::TOKEN_SELECTOR));
    harness.press("escape");
    assert!(!harness.dialog_open(), "hidden");
    assert!(!harness.drawn(crate::ui::kind_actions::token::TOKEN_SELECTOR));
    let refusal = harness
        .vcx
        .update(|_, cx| format!("{:?}", harness.panel.read(cx).refusal));
    assert!(!refusal.contains(TEST_TOKEN));
}

/// Task 7.1: Open Host opens the first rule's host over https.
#[gpui_kit::test]
async fn open_host_opens_the_first_rules_host(cx: &mut TestAppContext) {
    let ingress = json!({ "apiVersion": "networking.k8s.io/v1", "kind": "Ingress",
        "metadata": meta("web"),
        "spec": { "rules": [{ "host": "app.example.com" }, { "host": "api.example.com" }] } });
    let mut harness = open_objects(
        cx,
        kind("networking.k8s.io", "Ingress", "ingresses", true),
        vec![ingress],
    );
    harness.press("shift-o");
    assert!(!harness.dialog_open(), "no confirmation");
    assert_eq!(
        harness.vcx.opened_url().as_deref(),
        Some("https://app.example.com/")
    );
}

/// Review fix 4: a row menu's item acts on the row the menu opened on, even
/// once the selection is somewhere else.
#[gpui_kit::test]
async fn a_row_menu_item_acts_on_its_own_row(cx: &mut TestAppContext) {
    let ingress = |name: &str, host: &str| {
        json!({ "apiVersion": "networking.k8s.io/v1", "kind": "Ingress", "metadata": meta(name),
            "spec": { "rules": [{ "host": host }] } })
    };
    let mut harness = open_objects(
        cx,
        kind("networking.k8s.io", "Ingress", "ingresses", true),
        vec![
            ingress("api", "api.example.com"),
            ingress("web", "web.example.com"),
        ],
    );
    // The first row, `api`, is selected; the menu is the second row's.
    let entries = harness.vcx.update(|_, cx| {
        let panel = harness.panel.read(cx);
        let row = panel
            .table
            .as_ref()
            .expect("built")
            .read(cx)
            .delegate()
            .rows()
            .iter()
            .find(|row| row.object.name == "web")
            .expect("listed")
            .clone();
        harness
            .panel
            .update(cx, |panel, cx| panel.kind_menu(cx)(&row))
    });
    let open = entries
        .iter()
        .find(|entry| entry.label == "Open Host in Browser")
        .expect("offered");
    let run = open.run.clone();
    harness.vcx.update(|window, cx| run(window, cx));
    harness.vcx.run_until_parked();
    assert_eq!(
        harness.vcx.opened_url().as_deref(),
        Some("https://web.example.com/"),
        "the menu's row, not the selection"
    );
}

/// Task 8.1: nothing kind-specific is offered for a kind without actions.
#[gpui_kit::test]
async fn a_configmap_row_is_offered_no_kind_action(cx: &mut TestAppContext) {
    let mut harness = open(cx, kind("", "ConfigMap", "configmaps", true), &["settings"]);
    for gate in [
        "KindTriggerNow",
        "KindSuspend",
        "KindExpand",
        "KindAddTaint",
        "KindOpenHost",
    ] {
        assert!(!harness.in_context(gate), "{gate}");
    }
}
