//! Run from Library… and Save to Library… through a real window by keystrokes
//! (`exec-command-library` 4.1, 4.2): from the selected Pods row; and Save and
//! Run from a Deployment, which saves the entry and runs it in one of the
//! Deployment's pods.

use super::{Resource, save_from};
use crate::config::exec_library::{Target, WorkloadKind};
use crate::k8s::resource::pods::SelectedPod;
use crate::ui::exec_library::store::ExecLibraryStore;
use crate::ui::exec_library::test_support::{
    RUN_FROM_KEY, SAVE_FROM_KEY, dialog_open, entry, open, press, selector,
};
use gpui_kit::TestAppContext;

fn selected_pod(harness: &mut crate::ui::exec_library::test_support::Harness) -> String {
    harness.vcx.update(|_, cx| {
        cx.try_global::<SelectedPod>()
            .and_then(|selected| selected.0.clone())
            .expect("a pod is selected")
            .name
    })
}

/// Run from Library… on the selected pod: only what applies is offered, and
/// Enter runs it there.
#[gpui_kit::test]
async fn run_from_library_on_a_pod_runs_the_chosen_entry_there(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        vec![
            entry("Uptime", "uptime", Target::Any),
            entry("Db only", "psql", selector("app=db")),
        ],
    );
    press(&mut harness.vcx, "down");
    let pod = selected_pod(&mut harness);
    press(&mut harness.vcx, RUN_FROM_KEY);
    assert!(
        dialog_open(&mut harness.vcx),
        "the applicable entries are offered"
    );
    press(&mut harness.vcx, "enter");
    harness.settle();
    let sessions = harness.sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pod, pod, "run on the selected pod");
    assert_eq!(
        sessions[0]
            .command
            .as_ref()
            .map(|command| command.entry_name.as_str()),
        Some("Uptime")
    );
}

/// Spec "Create without the mouse": Save to Library… from the selected pod, a
/// name, Tab, a command, Enter - saved with the pod's prefill, and focus back
/// on the Pods panel.
#[gpui_kit::test]
async fn save_to_library_from_a_pod_without_the_mouse(cx: &mut TestAppContext) {
    let mut harness = open(cx, Vec::new());
    press(&mut harness.vcx, "down");
    press(&mut harness.vcx, SAVE_FROM_KEY);
    assert!(dialog_open(&mut harness.vcx), "the dialog opens");
    harness.vcx.simulate_input("Uptime");
    press(&mut harness.vcx, "tab");
    harness.vcx.simulate_input("uptime");
    press(&mut harness.vcx, "enter");
    assert!(!dialog_open(&mut harness.vcx), "saved and closed");
    let saved = harness
        .vcx
        .update(|_, cx| ExecLibraryStore::get(cx).entries.clone());
    assert_eq!(saved.len(), 1);
    assert!(
        matches!(&saved[0].target, Target::Selector { namespace: Some(namespace), selector }
            if namespace == "shop" && selector.starts_with("app=")),
        "prefilled from the pod: {:?}",
        saved[0].target
    );
    let pods_focused = harness.vcx.update(|window, cx| {
        harness
            .main
            .read(cx)
            .test_panel_view_for(&crate::ui::nav::NavTarget::pods(), cx)
            .is_some_and(|pods| pods.focus_handle(cx).contains_focused(window, cx))
    });
    assert!(pods_focused, "focus is back on the Pods panel");
}

/// Spec "Save and run from a Deployment": the entry is saved with that
/// Deployment as its target, and runs in one of its Running pods.
#[gpui_kit::test]
async fn save_and_run_from_a_deployment(cx: &mut TestAppContext) {
    let mut harness = open(cx, Vec::new());
    harness.vcx.update(|window, cx| {
        save_from(
            Resource::Workload {
                kind: WorkloadKind::Deployment,
                namespace: "shop".into(),
                name: "web".into(),
                selector: None,
            },
            "demo".into(),
            window,
            cx,
        )
    });
    harness.settle();
    harness.vcx.simulate_input("Web uptime");
    press(&mut harness.vcx, "tab");
    harness.vcx.simulate_input("uptime");
    press(&mut harness.vcx, "secondary-enter");
    harness.settle();
    let saved = harness
        .vcx
        .update(|_, cx| ExecLibraryStore::get(cx).entries.clone());
    assert_eq!(saved.len(), 1, "saved");
    assert_eq!(
        saved[0].target,
        Target::Workload {
            namespace: "shop".into(),
            workload_kind: WorkloadKind::Deployment,
            name: "web".into(),
        }
    );
    assert!(dialog_open(&mut harness.vcx), "three pods: it asks which");
    press(&mut harness.vcx, "enter");
    harness.settle();
    let sessions = harness.sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].pod, "web-1", "the Deployment's first pod");
}
