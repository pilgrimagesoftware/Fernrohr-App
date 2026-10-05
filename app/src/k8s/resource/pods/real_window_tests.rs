//! #136: `s` and `shift-f` on a Running pod in a real window - the Pods panel
//! inside `MainWindow`'s dock, with the app's keymap, over a fake cluster -
//! and the hint row listing both while they apply.

use super::actions::FAILURE_ID;
use super::hints::hint_selector;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::port_forwards::PortForwards;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::test_window::{PODS, temp_path};
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::util::shell::MainWindow;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Keystroke, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

const CONTEXT: &str = "real-window";

/// Running `shop/web-1`, one container, declaring `ports`.
fn running_pod(ports: &[u16]) -> Value {
    let ports: Vec<Value> = ports
        .iter()
        .map(|port| json!({ "containerPort": port }))
        .collect();
    json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": "web-1", "namespace": "shop", "uid": "u1" },
        "spec": { "containers": [{ "name": "app", "image": "nginx", "ports": ports }] },
        "status": { "phase": "Running", "containerStatuses": [{
            "name": "app", "image": "nginx", "imageID": "", "ready": true,
            "restartCount": 0, "state": { "running": {} } }] } })
}

struct Harness {
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

impl Harness {
    fn press(&mut self, key: &str) {
        self.vcx
            .simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
        self.vcx.run_until_parked();
    }

    fn drawn(&mut self, selector: String) -> bool {
        self.vcx.update(|window, cx| window.render_frame(cx));
        self.vcx.debug_bounds(selector.leak()).is_some()
    }

    fn forwards(&mut self) -> usize {
        self.vcx
            .update(|_, cx| PortForwards::entity(cx).read(cx).list().len())
    }

    fn open_targets(&mut self) -> Vec<NavTarget> {
        let main = self.main.clone();
        self.vcx.update(|_, cx| main.read(cx).test_open_targets())
    }
}

/// A window on the fake cluster holding `pod`, its Pods panel shown and
/// focused and the pod listed - not yet selected.
fn harness(cx: &mut TestAppContext, pod: Value) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, temp_path(), &temp_path());
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(PODS.0, PODS.1, pod);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client))
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec![CONTEXT.into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        main.update(cx, |main, cx| main.test_focus(window, cx));
    });
    let mut h = Harness { main, vcx };
    h.press("cmd-1");
    for _ in 0..400 {
        h.vcx.run_until_parked();
        if h.row_drawn() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h
}

impl Harness {
    fn row_drawn(&mut self) -> bool {
        self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window
                .try_find(gpui_kit::ElementId::NamedInteger("row".into(), 0))
                .is_some()
        })
    }
}

#[gpui_kit::test]
async fn shift_f_on_a_running_pod_forwards_its_port(cx: &mut TestAppContext) {
    let mut h = harness(cx, running_pod(&[18_084]));
    assert!(h.row_drawn(), "the pod is listed");
    assert!(
        !h.drawn(hint_selector("Port forward")),
        "nothing selected yet"
    );
    h.press("down");
    assert!(h.drawn(hint_selector("Port forward")), "the hint shows");
    assert!(
        h.drawn(hint_selector("Shell")),
        "a running container: Shell too"
    );
    h.press("shift-f");
    assert_eq!(h.forwards(), 1, "shift-f started the forward");
}

/// A running pod that declares no port says why it can't forward.
#[gpui_kit::test]
async fn shift_f_on_a_running_pod_without_ports_says_why(cx: &mut TestAppContext) {
    let mut h = harness(cx, running_pod(&[]));
    h.press("down");
    assert!(h.drawn(hint_selector("Port forward")));
    h.press("shift-f");
    assert_eq!(h.forwards(), 0);
    assert!(!h.vcx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(h.drawn(FAILURE_ID.to_string()), "it says it has no ports");
}

#[gpui_kit::test]
async fn s_on_a_running_pod_opens_a_shell(cx: &mut TestAppContext) {
    let mut h = harness(cx, running_pod(&[]));
    h.press("down");
    h.press("s");
    assert!(
        h.open_targets()
            .iter()
            .any(|target| matches!(target, NavTarget::Exec(exec) if exec.pod == "web-1")),
        "a shell panel opened: {:?}",
        h.open_targets()
    );
}
