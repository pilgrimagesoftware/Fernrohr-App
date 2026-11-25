//! Shell from the Pods panel, in a window with the app's keymap: offered - in
//! the palette and on `s` - only for a pod with a running container; straight
//! into the one running container, or a choice of which when there are several.

use crate::command::CommandRegistry;
use crate::k8s::resource::pods::test_window::{Harness, PODS, open};
use crate::util::shell::OpenExecSession;
use gpui_kit::SharedString;
use gpui_kit::TestAppContext;
use gpui_kit::component::WindowExt as _;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

/// The command ids the palette would offer now, from the focused element's
/// context stack, as `util::palette::open` reads it.
fn palette_ids(harness: &mut Harness) -> Vec<&'static str> {
    harness.vcx.update(|window, cx| {
        let contexts: Vec<SharedString> = window
            .context_stack()
            .iter()
            .filter_map(|context| context.primary().map(|entry| entry.key.clone()))
            .collect();
        let names: Vec<&str> = contexts.iter().map(|name| name.as_ref()).collect();
        cx.global::<CommandRegistry>()
            .available(&names)
            .iter()
            .map(|command| command.id)
            .collect()
    })
}

/// Records every shell the panel asks the window to open.
fn record_opens(harness: &mut Harness) -> Rc<RefCell<Vec<OpenExecSession>>> {
    let opened = Rc::new(RefCell::new(Vec::new()));
    harness.vcx.update(|_, cx| {
        let opened = opened.clone();
        cx.on_action(move |action: &OpenExecSession, _| opened.borrow_mut().push(action.clone()));
    });
    opened
}

fn press_dialog_button(harness: &mut Harness, n: usize) {
    for _ in 0..n {
        harness.vcx.simulate_keystrokes("tab");
        harness.vcx.run_until_parked();
    }
    let space = crate::util::keystroke::parse("space").expect("valid");
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

/// Spec "Shell unavailable for a pod with no running container": `web-3` is
/// pending with nothing running - Shell isn't in the palette and `s` does
/// nothing - while `web-1`, with its sidecar running, offers it.
#[gpui_kit::test]
async fn shell_is_offered_only_for_a_pod_with_a_running_container(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    assert!(
        palette_ids(&mut harness).contains(&"pods.shell"),
        "web-1 runs its sidecar"
    );

    harness.cluster.apply(
        PODS.0,
        PODS.1,
        json!({ "apiVersion": "v1", "kind": "Pod",
            "metadata": { "name": "web-3", "namespace": "shop", "uid": "u3" },
            "spec": { "containers": [{ "name": "app", "image": "shop/web:1.4" }] },
            "status": { "phase": "Pending" } }),
    );
    harness.wait_for("listed web-3", |panel, cx| {
        panel.table.read(cx).pods().len() == 3
    });
    harness.press("down");
    harness.press("down");
    assert_eq!(harness.selected().as_deref(), Some("web-3"));
    let opened = record_opens(&mut harness);

    assert!(
        !palette_ids(&mut harness).contains(&"pods.shell"),
        "not offered"
    );
    harness.press("s");
    assert!(opened.borrow().is_empty(), "and `s` does nothing");
    assert!(
        !harness
            .vcx
            .update(|window, cx| window.has_active_dialog(cx))
    );
}

/// Spec "Open a shell into a running pod": one running container - `web-1`'s
/// sidecar, its app crash-looping - opens straight into it.
#[gpui_kit::test]
async fn one_running_container_opens_straight_into_it(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    let opened = record_opens(&mut harness);

    harness.press("s");

    let opened = opened.borrow();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].target.pod, "web-1");
    assert_eq!(opened[0].target.container, "sidecar");
    assert_eq!(
        opened[0].context_name,
        crate::k8s::resource::pods::test_window::CONTEXT
    );
}

/// Spec "Multi-container pod prompts for a container": `web-2` runs two, so
/// `s` asks which - Cancel opens nothing, and choosing one opens that one.
#[gpui_kit::test]
async fn several_running_containers_ask_which(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.press("down");
    assert_eq!(harness.selected().as_deref(), Some("web-2"));
    let opened = record_opens(&mut harness);

    harness.press("s");
    assert!(
        harness
            .vcx
            .update(|window, cx| window.has_active_dialog(cx)),
        "it asks"
    );
    // The buttons run app, sidecar, Cancel.
    press_dialog_button(&mut harness, 3);
    assert!(opened.borrow().is_empty(), "Cancel opens nothing");

    harness.press("s");
    press_dialog_button(&mut harness, 2);
    let opened = opened.borrow();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].target.pod, "web-2");
    assert_eq!(opened[0].target.container, "sidecar", "the second button");
}

/// #398: a pod whose second long-running container is a sidecar - an init
/// container with `restartPolicy: Always` - offers it, marked, and `s` then
/// the second button opens a shell in it.
#[gpui_kit::test]
async fn a_running_sidecar_is_offered_and_opened(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    let mut pod = serde_json::to_value(crate::k8s::resource::pods::sidecar_fixture()).unwrap();
    pod["apiVersion"] = json!("v1");
    pod["kind"] = json!("Pod");
    pod["metadata"] = json!({ "name": "web-3", "namespace": "shop", "uid": "u3" });
    harness.cluster.apply(PODS.0, PODS.1, pod);
    harness.wait_for("listed the sidecar pod", |panel, cx| {
        panel.table.read(cx).pods().len() == 3
    });
    harness.press("down");
    harness.press("down");
    assert_eq!(harness.selected().as_deref(), Some("web-3"));
    let selection = harness.vcx.update(|_, cx| {
        cx.global::<crate::k8s::resource::pods::SelectedPod>()
            .0
            .clone()
            .expect("a selection")
    });
    assert_eq!(selection.containers, ["app", "proxy"]);
    assert_eq!(selection.container_label("proxy"), "proxy (sidecar)");

    let opened = record_opens(&mut harness);
    harness.press("s");
    // The buttons run app, proxy (sidecar), Cancel.
    press_dialog_button(&mut harness, 2);
    let opened = opened.borrow();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].target.container, "proxy");
}
