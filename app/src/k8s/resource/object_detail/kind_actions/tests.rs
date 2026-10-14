//! Kind-specific actions from a detail panel, by their default keys, in a
//! window with the app's keymap over a fake cluster that watches, stores
//! creates and merges patches: what the shown object is offered, an action run
//! from there, a Service's Port Forward reaching the shared forward flow, and
//! a token shown once.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_detail::ObjectDetailPanel;
use crate::k8s::resource::object_detail::fetch::ObjectDetailState;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::nav::ObjectTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};
use kube::core::GroupVersionKind;
use serde_json::{Value, json};

const CONTEXT: &str = "kind-dev";

struct Harness {
    cluster: FakeCluster,
    panel: Entity<ObjectDetailPanel>,
    vcx: VisualTestContext,
}

fn kind(group: &str, kind: &str, plural: &str) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: plural.into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// The panel open and focused on `object` of `kind`, in `shop`, once loaded.
fn open(cx: &mut TestAppContext, kind: DiscoveredKind, object: Value) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_detail::register_commands(&mut registry);
        crate::ui::confirm_dialog::register_commands(&mut registry);
        crate::ui::kind_actions::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let (cluster, client) = FakeCluster::start(cx);
    let prefix = if kind.gvk.group.is_empty() {
        "/api/v1".to_string()
    } else {
        format!("/apis/{}/v1", kind.gvk.group)
    };
    let name = object["metadata"]["name"].as_str().unwrap().to_string();
    cluster.apply(&prefix, &kind.plural, object);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
        DiscoveryRegistry::insert_test(cx, CONTEXT, vec![kind.clone()]);
    });
    let target = ObjectTarget {
        kind,
        namespace: Some("shop".into()),
        name,
    };
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Object(target.clone()), CONTEXT.into());
        let panel = cx.new(|cx| ObjectDetailPanel::new(target, scope, cx));
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
    harness.wait_for("loaded the object", |panel| {
        matches!(panel.state, ObjectDetailState::Loaded(..))
    });
    let panel = harness.panel.clone();
    harness.vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle(cx).focus(window, cx);
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

    fn in_context(&mut self, context: &str) -> bool {
        self.vcx.update(|window, cx| {
            window.render_frame(cx);
            window
                .context_stack()
                .iter()
                .any(|stack| stack.contains(context))
        })
    }

    fn wait_for(&mut self, what: &str, done: impl Fn(&ObjectDetailPanel) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.vcx.update(|_, cx| done(self.panel.read(cx))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }
}

fn meta(name: &str) -> Value {
    json!({ "name": name, "namespace": "shop", "uid": format!("uid-{name}") })
}

/// Tasks 2.3 and 8.1: a CronJob's panel offers Trigger Now and Suspend - and
/// no other kind's actions - and `p` suspends it, on the listed object only.
#[gpui_kit::test]
async fn a_cronjobs_panel_suspends_it(cx: &mut TestAppContext) {
    let cronjob = json!({ "apiVersion": "batch/v1", "kind": "CronJob", "metadata": meta("backup"),
        "spec": { "schedule": "0 3 * * *", "jobTemplate": { "spec": {} } } });
    let mut harness = open(cx, kind("batch", "CronJob", "cronjobs"), cronjob);
    assert!(harness.in_context("KindTriggerNow"));
    assert!(harness.in_context("KindSuspend"));
    for other in [
        "KindPauseRollout",
        "KindExpand",
        "KindForwardService",
        "KindCreateToken",
    ] {
        assert!(!harness.in_context(other), "{other}");
    }
    harness.press("p");
    assert!(harness.dialog_open(), "it asks first");
    harness.press("enter");
    for _ in 0..400 {
        if !harness.cluster.patches().is_empty() {
            break;
        }
        harness.vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        harness.cluster.patches()[0].1,
        json!({ "metadata": { "uid": "uid-backup" }, "spec": { "suspend": true } })
    );
}

/// Task 1.2: a Service's panel offers Port Forward, which runs the Services
/// list's own flow - here, reporting that the Service has no ports.
#[gpui_kit::test]
async fn a_services_panel_forwards_it_through_the_shared_flow(cx: &mut TestAppContext) {
    let service = json!({ "apiVersion": "v1", "kind": "Service", "metadata": meta("web"),
        "spec": { "selector": { "app": "web" } } });
    let mut harness = open(cx, kind("", "Service", "services"), service);
    assert!(harness.in_context("KindForwardService"));
    harness.press("shift-f");
    for _ in 0..400 {
        harness.vcx.run_until_parked();
        let notified = harness.vcx.update(|_, cx| {
            cx.try_global::<crate::k8s::resource::port_forwarding::NotifiedFailures>()
                .map(|failures| failures.0.clone())
                .unwrap_or_default()
        });
        if let Some((title, reason)) = notified.first() {
            assert_eq!(title, "Couldn't forward web");
            assert!(reason.contains("has no ports"), "{reason}");
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the forward flow never ran");
}

/// Task 6.2: a ServiceAccount's panel creates a token and shows it once;
/// Escape drops it, and the panel holds none of it.
#[gpui_kit::test]
async fn a_serviceaccounts_panel_shows_a_new_token_once(cx: &mut TestAppContext) {
    let account =
        json!({ "apiVersion": "v1", "kind": "ServiceAccount", "metadata": meta("deploy") });
    let mut harness = open(cx, kind("", "ServiceAccount", "serviceaccounts"), account);
    harness.press("shift-k");
    for _ in 0..400 {
        if harness.dialog_open() {
            break;
        }
        harness.vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(harness.dialog_open(), "the token is shown");
    assert_eq!(harness.cluster.creates()[0].0, "token");
    harness.press("escape");
    assert!(!harness.dialog_open(), "and gone");
    let held = harness
        .vcx
        .update(|_, cx| format!("{:?}", harness.panel.read(cx).refusal));
    assert!(!held.contains(crate::k8s::test_cluster::TEST_TOKEN));
}
