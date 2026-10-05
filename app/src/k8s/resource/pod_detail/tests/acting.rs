//! Acting on the panel's own pod from the keyboard (`k9s-remaining-keybindings`,
//! issues #136 and #143): `ctrl-d` asks then deletes, `ctrl-k` kills at once,
//! `s` opens a shell while a container runs, and `shift-f` forwards its port -
//! the Pods list's actions and flows, in a real window with the panel's keymap
//! over a fake cluster. After a delete the panel shows the pod gone, not its
//! last state as if current.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::port_forwards::{PortForwardRequest, PortForwards};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pod_detail::actions::{NOTICE_ID, hint_selector};
use crate::k8s::resource::pod_detail::model::DetailView;
use crate::k8s::resource::pod_detail::panel::PodDetailPanel;
use crate::k8s::resource::pod_detail::register_commands;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::KeymapConfig;
use crate::ui::detail::lifecycle::Lifecycle;
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use crate::util::shell::OpenExecSession;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;

const CONTEXT: &str = "kind-dev";

/// `shop/web-1`, its one container `app` declaring `port` - running, or not.
fn pod(running: bool, port: u16) -> Value {
    let state = if running {
        json!({ "running": { "startedAt": "2026-01-01T00:00:00Z" } })
    } else {
        json!({ "waiting": { "reason": "ContainerCreating" } })
    };
    json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": "web-1", "namespace": "shop", "uid": "u1" },
        "spec": { "containers": [{ "name": "app", "image": "nginx",
            "ports": [{ "containerPort": port, "name": "http" }] }] },
        "status": {
            "phase": if running { "Running" } else { "Pending" },
            "containerStatuses": [{ "name": "app", "image": "nginx", "imageID": "",
                "ready": running, "restartCount": 0, "state": state }],
        },
    })
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<PodDetailPanel>,
    vcx: VisualTestContext,
}

/// The panel on `shop/web-1`, held by the fake cluster as `initial`, in a
/// window with a `Root` (for its dialogs), its keys bound and focus in it,
/// once it shows the pod.
fn open(cx: &mut TestAppContext, initial: Value) -> Harness {
    open_with(cx, initial, None)
}

/// [`open`], with discovery reporting `kinds` for the context when given.
fn open_with(
    cx: &mut TestAppContext,
    initial: Value,
    kinds: Option<Vec<crate::k8s::cluster::discovery::DiscoveredKind>>,
) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        cx.set_global(registry);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "pods", initial);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
        if let Some(kinds) = kinds {
            crate::k8s::cluster::discovery_registry::DiscoveryRegistry::insert_test(
                cx, CONTEXT, kinds,
            );
        }
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let pod = PodRef {
            namespace: "shop".into(),
            name: "web-1".into(),
        };
        let scope = PanelScope::new(NavTarget::pod("shop", "web-1"), CONTEXT.into());
        let panel = cx.new(|cx| PodDetailPanel::new(pod, scope, DetailView::Structured, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    let mut harness = Harness {
        cluster,
        panel,
        vcx,
    };
    harness.wait_for("showed the pod", |panel, cx| {
        panel.pod().is_some() && panel.live_synced(cx)
    });
    let panel = harness.panel.clone();
    harness.vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle.clone().focus(window, cx);
    });
    harness.vcx.run_until_parked();
    harness
}

impl Harness {
    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    fn wait_for(&mut self, what: &str, done: impl Fn(&PodDetailPanel, &gpui_kit::App) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.vcx.update(|_, cx| done(self.panel.read(cx), cx)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }

    fn wait_for_delete(&mut self) -> Vec<(String, Value)> {
        for _ in 0..400 {
            if !self.cluster.deletes().is_empty() {
                break;
            }
            self.vcx.run_until_parked();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        self.cluster.deletes()
    }

    /// Whether the hint labelled `label` is drawn.
    fn hint_shown(&mut self, label: &str) -> bool {
        self.vcx.update(|window, cx| window.render_frame(cx));
        // `debug_bounds` takes a `'static` selector; a test's few leak nothing
        // that matters.
        let selector: &'static str = Box::leak(hint_selector(label).into_boxed_str());
        self.vcx.debug_bounds(selector).is_some()
    }
}

fn deleted(panel: &PodDetailPanel) -> bool {
    matches!(panel.notice, Some(Lifecycle::Deleted { .. }))
}

/// `ctrl-d` asks first; Escape sends nothing; `ctrl-d` then Enter deletes the
/// pod with its own grace period, and the panel shows it deleted - offering
/// no second delete.
#[gpui_kit::test]
async fn ctrl_d_asks_then_deletes_and_the_panel_shows_the_pod_gone(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod(true, 18_091));
    assert!(harness.hint_shown("Delete"), "Delete's key is in the hints");

    harness.press("ctrl-d");
    assert!(harness.dialog_open(), "Delete asks first");
    harness.press("escape");
    assert!(harness.cluster.deletes().is_empty(), "Escape sends nothing");

    harness.press("ctrl-d");
    harness.press("enter");
    let deletes = harness.wait_for_delete();
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].0, "web-1");
    assert_eq!(
        deletes[0].1.get("gracePeriodSeconds"),
        None,
        "the pod's own grace period"
    );
    harness.wait_for("showed the pod deleted", |panel, _| deleted(panel));
    assert!(
        !harness.hint_shown("Delete"),
        "a deleted pod offers no Delete"
    );
    harness.press("ctrl-d");
    assert!(!harness.dialog_open(), "nor asks again");
}

/// `ctrl-k` kills at once: no question, a zero grace period, and the panel
/// shows the pod deleted.
#[gpui_kit::test]
async fn ctrl_k_kills_the_pod_at_once(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod(true, 18_092));

    harness.press("ctrl-k");
    assert!(!harness.dialog_open(), "no confirmation");
    let deletes = harness.wait_for_delete();
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].1["gracePeriodSeconds"], json!(0));
    harness.wait_for("showed the pod deleted", |panel, _| deleted(panel));
}

/// `s` opens a shell in the pod's running container, and its hint shows
/// after Logs; a pod with nothing running offers neither.
#[gpui_kit::test]
async fn s_opens_a_shell_only_while_a_container_runs(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod(true, 18_093));
    let opened = Rc::new(RefCell::new(Vec::new()));
    harness.vcx.update(|_, cx| {
        let opened = opened.clone();
        cx.on_action(move |action: &OpenExecSession, _| opened.borrow_mut().push(action.clone()));
    });
    assert!(harness.hint_shown("Shell"), "Shell's key is in the hints");

    harness.press("s");
    let shells = opened.borrow().clone();
    assert_eq!(shells.len(), 1, "one shell opened");
    assert_eq!(shells[0].context_name, CONTEXT);
    assert_eq!(
        (
            shells[0].target.namespace.as_str(),
            shells[0].target.pod.as_str(),
            shells[0].target.container.as_str()
        ),
        ("shop", "web-1", "app")
    );

    harness.cluster.apply("/api/v1", "pods", pod(false, 18_093));
    harness.wait_for("showed nothing running", |panel, _| {
        panel.running().is_empty()
    });
    assert!(!harness.hint_shown("Shell"), "no Shell hint");
    harness.press("s");
    assert_eq!(opened.borrow().len(), 1, "`s` opened nothing more");
}

/// `shift-f` forwards the pod's one port straight away and says where it
/// listens; its hint shows.
#[gpui_kit::test]
async fn shift_f_forwards_the_pods_port(cx: &mut TestAppContext) {
    let mut harness = open(cx, pod(true, 18_094));
    assert!(
        harness.hint_shown("Port forward"),
        "its key is in the hints"
    );

    harness.press("shift-f");
    assert!(!harness.dialog_open(), "one port needs no question");
    let forwards: Vec<PortForwardRequest> = harness.vcx.update(|_, cx| {
        PortForwards::entity(cx)
            .read(cx)
            .list()
            .into_iter()
            .map(|(request, _, _)| request)
            .collect()
    });
    assert_eq!(
        forwards,
        [PortForwardRequest {
            context_name: CONTEXT.into(),
            namespace: "shop".into(),
            pod: "web-1".into(),
            remote_port: 18_094,
        }]
    );
    harness.vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        harness.vcx.debug_bounds(NOTICE_ID).is_some(),
        "it says where it listens"
    );
}

/// Discovery lists no `delete` for Pods: the panel offers neither Delete nor
/// Kill - no hint, and `ctrl-d` and `ctrl-k` send nothing.
#[gpui_kit::test]
async fn a_pod_kind_without_delete_offers_no_delete_or_kill(cx: &mut TestAppContext) {
    use crate::k8s::cluster::discovery::{DiscoveredKind, KindVerbs};
    let pods = DiscoveredKind {
        verbs: KindVerbs {
            delete: false,
            ..KindVerbs::default()
        },
        ..DiscoveredKind::pods()
    };
    let mut harness = open_with(cx, pod(true, 18_095), Some(vec![pods]));
    assert!(!harness.hint_shown("Delete"), "no Delete hint");

    harness.press("ctrl-d");
    assert!(!harness.dialog_open(), "no question");
    harness.press("ctrl-k");
    harness.vcx.run_until_parked();
    assert!(harness.cluster.deletes().is_empty(), "nothing deleted");
}
