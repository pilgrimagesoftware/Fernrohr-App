//! Port-forward from the Pods panel, in a window with the app's keymap over a
//! fake cluster: one declared port forwards straight away, several ask which -
//! Cancel forwarding nothing - and a pod with none says so. A started forward
//! is in the app's list keyed by the same request Manage Tunnels stops it by.

use super::super::actions::{FAILURE_ID, NOTICE_ID};
use crate::k8s::cluster::port_forwards::{PortForwardRequest, PortForwards};
use crate::k8s::resource::pods::test_window::{CONTEXT, Harness, PODS, open};
use gpui_kit::TestAppContext;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use serde_json::{Value, json};

/// `shop/<name>`, Running, its containers declaring `ports` - (container, port, name).
fn pod_with_ports(uid: &str, name: &str, ports: &[(&str, u16, &str)]) -> Value {
    let mut containers: Vec<Value> = Vec::new();
    for (container, port, port_name) in ports {
        containers.push(json!({ "name": container, "image": "nginx",
            "ports": [{ "containerPort": port, "name": port_name }] }));
    }
    json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": name, "namespace": "shop", "uid": uid },
        "spec": { "containers": containers },
        "status": { "phase": "Running" } })
}

fn forwards(harness: &mut Harness) -> Vec<PortForwardRequest> {
    harness.vcx.update(|_, cx| {
        PortForwards::entity(cx)
            .read(cx)
            .list()
            .into_iter()
            .map(|(request, _, _)| request)
            .collect()
    })
}

fn request(pod: &str, port: u16) -> PortForwardRequest {
    PortForwardRequest {
        context_name: CONTEXT.into(),
        namespace: "shop".into(),
        pod: pod.into(),
        remote_port: port,
    }
}

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

/// Spec "Start a port-forward from a pod row", 4.1 and 4.2's single-port
/// shortcut: one declared port forwards straight away, into the app's list,
/// keyed exactly as Manage Tunnels stops it.
#[gpui_kit::test]
async fn one_port_forwards_straight_away(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.cluster.apply(
        PODS.0,
        PODS.1,
        pod_with_ports("u1", "web-1", &[("app", 18_081, "http")]),
    );
    harness.wait_for("listed web-1's port", |panel, cx| {
        panel.table.read(cx).pods().iter().any(|pod| {
            pod.spec
                .as_ref()
                .is_some_and(|spec| spec.containers[0].ports.is_some())
        })
    });

    harness.press("shift-f");

    assert!(
        !harness
            .vcx
            .update(|window, cx| window.has_active_dialog(cx)),
        "no prompt"
    );
    assert_eq!(forwards(&mut harness), [request("web-1", 18_081)]);
    harness.vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        harness.vcx.debug_bounds(NOTICE_ID).is_some(),
        "it says where it listens"
    );
}

/// Spec "Multiple ports on the target", 4.2: two ports ask which - Cancel
/// forwards nothing, and the second button forwards the second port.
#[gpui_kit::test]
async fn several_ports_ask_which(cx: &mut TestAppContext) {
    let mut harness = open(cx);
    harness.cluster.apply(
        PODS.0,
        PODS.1,
        pod_with_ports(
            "u1",
            "web-1",
            &[("app", 18_082, "http"), ("sidecar", 19_901, "admin")],
        ),
    );
    harness.wait_for("listed web-1's ports", |panel, cx| {
        panel.table.read(cx).pods().iter().any(|pod| {
            pod.spec.as_ref().is_some_and(|spec| {
                spec.containers.len() == 2 && spec.containers[1].ports.is_some()
            })
        })
    });

    harness.press("shift-f");
    assert!(
        harness
            .vcx
            .update(|window, cx| window.has_active_dialog(cx)),
        "it asks"
    );
    // The buttons run 18082, 19901, Cancel.
    press_dialog_button(&mut harness, 3);
    assert!(forwards(&mut harness).is_empty(), "Cancel forwards nothing");

    harness.press("shift-f");
    press_dialog_button(&mut harness, 2);
    assert_eq!(forwards(&mut harness), [request("web-1", 19_901)]);
}

/// A pod that declares no ports has nothing to forward: it says so.
#[gpui_kit::test]
async fn a_pod_without_ports_says_so(cx: &mut TestAppContext) {
    let mut harness = open(cx);

    harness.press("shift-f");

    assert!(forwards(&mut harness).is_empty());
    let failure = harness
        .vcx
        .update(|_, cx| harness.panel.read(cx).action_failure.clone());
    assert!(failure.is_some_and(|failure| {
        failure
            .failure
            .message
            .contains("declares no container ports")
    }));
    harness.vcx.update(|window, cx| window.render_frame(cx));
    assert!(harness.vcx.debug_bounds(FAILURE_ID).is_some());
}
