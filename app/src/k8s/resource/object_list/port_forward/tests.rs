//! Port-forward from a Services list, in a window with the list's keymap over a
//! fake cluster: `shift-f` reads the Service's ports, asks which of its two,
//! and forwards the chosen one to the Running Pod it selects, on the port its
//! named `targetPort` resolves to.

use super::super::panel::ObjectListPanel;
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::port_forwards::{PortForwardRequest, PortForwards};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_list::ObjectsTable;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{AppContext as _, Focusable as _, Keystroke, TestAppContext, VisualTestContext};
use kube::api::DynamicObject;
use kube::core::GroupVersionKind;
use kube_runtime::watcher;
use serde_json::json;

const CONTEXT: &str = "kind-dev";

fn services() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

fn service() -> serde_json::Value {
    json!({ "apiVersion": "v1", "kind": "Service",
        "metadata": { "name": "web", "namespace": "shop", "uid": "svc" },
        "spec": { "selector": { "app": "web" }, "ports": [
            { "name": "http", "port": 80, "targetPort": "http" },
            { "name": "admin", "port": 9000, "targetPort": 9901 },
        ] } })
}

fn press(vcx: &mut VisualTestContext, keys: &str) {
    for key in keys.split(' ') {
        vcx.simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
    }
    vcx.run_until_parked();
}

fn press_dialog_button(vcx: &mut VisualTestContext, n: usize) {
    for _ in 0..n {
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
    }
    let space = Keystroke::parse("space").expect("valid");
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    vcx.run_until_parked();
}

/// Spec "Multiple ports on the target" for a Service, and 4.1's "equivalent
/// Services row": the chosen Service port forwards to its Pod's resolved port.
#[gpui_kit::test]
async fn a_service_forwards_its_chosen_port_to_a_running_pod(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_list::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        // Up and Down, as the app binds them for list panels.
        cx.bind_keys(crate::ui::list_keys::bindings(&[
            crate::k8s::resource::object_list::LIST_KEY_CONTEXT,
        ]));
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "services", service());
    cluster.apply(
        "/api/v1",
        "pods",
        json!({ "apiVersion": "v1", "kind": "Pod",
            "metadata": { "name": "web-1", "namespace": "shop", "uid": "p1", "labels": { "app": "web" } },
            "spec": { "containers": [{ "name": "app", "image": "nginx",
                "ports": [{ "containerPort": 18_083, "name": "http" }] }] },
            "status": { "phase": "Running" } }),
    );
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            CONTEXT,
            ConnectionState::Connected(client.clone()),
        );
    });
    let table = cx.update(|cx| {
        cx.new(|_| {
            let mut table = ObjectsTable::for_kind(&services());
            let object: DynamicObject = serde_json::from_value(service()).unwrap();
            table.apply(watcher::Event::Apply(object));
            table
        })
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(services()), CONTEXT.into());
        let panel = cx.new(|cx| ObjectListPanel::with_table(services(), scope, table, client, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    vcx.run_until_parked();

    press(&mut vcx, "down");
    press(&mut vcx, "shift-f");
    for _ in 0..400 {
        if vcx.update(|window, cx| window.has_active_dialog(cx)) {
            break;
        }
        vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        vcx.update(|window, cx| window.has_active_dialog(cx)),
        "it asks which port"
    );

    // The buttons run 80, 9000, Cancel.
    press_dialog_button(&mut vcx, 1);
    let expected = PortForwardRequest {
        context_name: CONTEXT.into(),
        namespace: "shop".into(),
        pod: "web-1".into(),
        remote_port: 18_083,
    };
    let mut listed = Vec::new();
    for _ in 0..400 {
        vcx.run_until_parked();
        listed = vcx.update(|_, cx| {
            PortForwards::entity(cx)
                .read(cx)
                .list()
                .into_iter()
                .map(|(request, _, _)| request)
                .collect::<Vec<_>>()
        });
        if !listed.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        listed,
        [expected],
        "port 80's `http` target is the pod's 18083"
    );

    // `port-forward-indicators` 2.1: the Service's own row shows it.
    vcx.update(|window, cx| {
        use gpui_kit::test::TestWindowExt as _;
        window.render_frame(cx)
    });
    let selector: &'static str = crate::ui::forward_indicator::selector("web").leak();
    assert!(
        vcx.debug_bounds(selector).is_some(),
        "the Service's row shows its forward"
    );
}
