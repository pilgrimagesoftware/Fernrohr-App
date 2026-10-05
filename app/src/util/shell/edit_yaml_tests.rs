//! #140: Edit YAML (`e`) from every place it applies, through a real window
//! and the app's keymap over a fake cluster - a list panel's selected row (an
//! object list and the Pods panel) and a pod's detail panel. Each opens the
//! object's detail panel on the YAML with the editor open; a Secret's says why
//! it can't be edited instead.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::{NavTarget, OpenedPanel};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Keystroke, TestAppContext, VisualTestContext};
use kube::core::GroupVersionKind;
use serde_json::{Value, json};

const CONTEXT: &str = "edit-yaml";

fn kind(group: &str, kind: &str, plural: &str) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: plural.into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

fn object(api_version: &str, kind: &str, name: &str) -> Value {
    json!({ "apiVersion": api_version, "kind": kind,
        "metadata": { "name": name, "namespace": "shop", "uid": format!("u-{name}") } })
}

fn pod() -> Value {
    json!({ "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": "web-1", "namespace": "shop", "uid": "u-web-1" },
        "spec": { "containers": [{ "name": "app", "image": "nginx" }] },
        "status": { "phase": "Running" } })
}

struct Harness {
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

/// A window over a fake cluster holding a Deployment, a Secret and a Pod, all
/// in `shop`, showing `target`, focused.
fn harness(cx: &mut TestAppContext, target: NavTarget) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/apis/apps/v1",
        "deployments",
        object("apps/v1", "Deployment", "web"),
    );
    cluster.apply("/api/v1", "secrets", object("v1", "Secret", "creds"));
    cluster.apply("/api/v1", "pods", pod());
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client))
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| {
            let mut main = MainWindow::test_workspace(vec![CONTEXT.into()], window, cx);
            main.open_target(target, window, cx);
            main
        });
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, _| window.activate_window());
    Harness { main, vcx }
}

impl Harness {
    fn press(&mut self, key: &str) {
        self.vcx
            .simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
        self.vcx.run_until_parked();
    }

    /// Waits (in real time - the fake cluster answers on its own thread)
    /// until `done`.
    fn wait_for(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if done(self) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }

    fn row_drawn(&mut self) -> bool {
        self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window
                .try_find(gpui_kit::ElementId::NamedInteger("row".into(), 0))
                .is_some()
        })
    }

    /// The open object detail panel over `name`: whether it's editing, and
    /// its notice.
    fn object_panel(&mut self, name: &str) -> Option<(bool, Option<String>)> {
        let main = self.main.clone();
        self.vcx.update(|_, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
                return None;
            };
            open_panels
                .iter()
                .find_map(|open| match (&open.key.target, &open.panel) {
                    (NavTarget::Object(target), Some(OpenedPanel::ObjectDetail(panel)))
                        if target.name == name =>
                    {
                        Some(panel.read(cx).test_edit_state())
                    }
                    _ => None,
                })
        })
    }

    /// Selects the list's first row and presses `e`, then waits for the
    /// object's panel to load and settle.
    fn edit_first_row(&mut self, name: &str) -> (bool, Option<String>) {
        self.wait_for("listed the object", |h| h.row_drawn());
        self.press("down");
        self.press("e");
        let mut state = None;
        self.wait_for("opened the object's panel and settled", |h| {
            state = h.object_panel(name);
            matches!(&state, Some((true, _)) | Some((false, Some(_))))
        });
        state.expect("the object's panel")
    }
}

#[gpui_kit::test]
async fn e_on_an_object_list_row_edits_its_yaml(cx: &mut TestAppContext) {
    let mut h = harness(
        cx,
        NavTarget::Kind(kind("apps", "Deployment", "deployments")),
    );
    assert_eq!(h.edit_first_row("web"), (true, None));
}

#[gpui_kit::test]
async fn e_on_a_pods_row_edits_its_yaml(cx: &mut TestAppContext) {
    let mut h = harness(cx, NavTarget::pods());
    assert_eq!(h.edit_first_row("web-1"), (true, None));
}

#[gpui_kit::test]
async fn e_in_a_pods_detail_panel_edits_its_yaml(cx: &mut TestAppContext) {
    let mut h = harness(cx, NavTarget::pod("shop", "web-1"));
    let main = h.main.clone();
    h.vcx
        .update(|window, cx| main.update(cx, |main, cx| main.test_focus(window, cx)));
    h.vcx.run_until_parked();
    // Focus the pod's panel itself: it's the window's only panel.
    let focus = h.vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            panic!("a workspace");
        };
        open_panels
            .iter()
            .find_map(|open| match &open.panel {
                Some(panel @ OpenedPanel::PodDetail(_)) => Some(panel.focus_handle(cx)),
                _ => None,
            })
            .expect("the pod's detail panel")
    });
    h.vcx.update(|window, cx| focus.focus(window, cx));
    h.press("e");
    let mut state = None;
    h.wait_for("opened the pod's object panel and settled", |h| {
        state = h.object_panel("web-1");
        matches!(&state, Some((true, _)))
    });
}

/// Every kind but a Secret: a Secret's panel says why, and opens no editor.
#[gpui_kit::test]
async fn e_on_a_secret_row_says_why_it_cannot(cx: &mut TestAppContext) {
    let mut h = harness(cx, NavTarget::Kind(kind("", "Secret", "secrets")));
    let (editing, notice) = h.edit_first_row("creds");
    assert!(!editing);
    assert!(notice.is_some_and(|notice| notice.contains("Secret")));
}
