//! Bulk View Logs through a real window (`bulk-select-list-actions` 7.7): the
//! Pods panel's checked pods each get a Logs panel of their own - at once up to
//! five, past that only once confirmed. The command is bound to a key in a test
//! `keymap.toml`; it has none by default.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::{NavTarget, OpenedPanel};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::json;

const VIEW_LOGS_KEY: &str = "f12";

struct Harness {
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

/// A workspace on `demo` over a fake cluster holding `count` pods, its Pods
/// panel focused with the first pod selected.
fn open(cx: &mut TestAppContext, count: usize) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    std::fs::write(
        &keymap,
        format!("[bindings]\n\"bulk.view_logs\" = \"{VIEW_LOGS_KEY}\"\n"),
    )
    .expect("the test keymap is written");
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
    });
    let (cluster, client) = FakeCluster::start(cx);
    for n in 0..count {
        cluster.apply(
            "/api/v1",
            "pods",
            json!({ "apiVersion": "v1", "kind": "Pod",
                "metadata": { "name": format!("web-{n}"), "namespace": "shop",
                    "uid": format!("u{n}") },
                "spec": { "containers": [{ "name": "app" }] },
                "status": { "phase": "Running" } }),
        );
    }
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connected(client));
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let pods = vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return None;
        };
        open_panels.iter().find_map(|open| match &open.panel {
            Some(OpenedPanel::Pods(panel)) => Some(panel.clone()),
            _ => None,
        })
    });
    let pods = pods.expect("the workspace lands on Pods");
    for _ in 0..400 {
        vcx.run_until_parked();
        if vcx.update(|_, cx| pods.read(cx).test_rows(cx) == count) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        window.activate_window();
        let handle = pods.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    });
    let mut harness = Harness { main, vcx };
    harness.press("down");
    harness
}

impl Harness {
    fn press(&mut self, key: &str) {
        self.vcx.simulate_keystrokes(key);
        self.vcx.run_until_parked();
    }

    fn check(&mut self, count: usize) {
        for n in 0..count {
            if n > 0 {
                self.press("down");
            }
            self.press("x");
        }
    }

    /// The pods with a Logs panel of their own open.
    fn logs_panels(&mut self) -> Vec<String> {
        self.vcx.update(|_, cx| {
            let WindowMode::Workspace { open_panels, .. } = &self.main.read(cx).mode else {
                return Vec::new();
            };
            let mut pods: Vec<String> = open_panels
                .iter()
                .filter_map(|open| match &open.key.target {
                    NavTarget::PodLogs(pod) => Some(pod.name.clone()),
                    _ => None,
                })
                .collect();
            pods.sort();
            pods
        })
    }

    fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }
}

/// Three checked pods: three Logs panels, one per pod, with no question.
#[gpui_kit::test]
async fn three_checked_pods_open_three_logs_panels_at_once(cx: &mut TestAppContext) {
    let mut harness = open(cx, 4);
    harness.check(3);
    harness.press(VIEW_LOGS_KEY);
    assert!(!harness.dialog_open(), "no confirmation at three");
    assert_eq!(harness.logs_panels(), ["web-0", "web-1", "web-2"]);
}

/// Eight: it asks first; canceling opens none, confirming opens all eight.
#[gpui_kit::test]
async fn eight_checked_pods_ask_before_opening_eight_logs_panels(cx: &mut TestAppContext) {
    let mut harness = open(cx, 8);
    harness.check(8);
    harness.press(VIEW_LOGS_KEY);
    assert!(harness.dialog_open(), "eight asks first");
    harness.press("escape");
    assert!(harness.logs_panels().is_empty(), "canceling opens none");

    harness.press(VIEW_LOGS_KEY);
    harness.press("enter");
    assert_eq!(harness.logs_panels().len(), 8);
}
