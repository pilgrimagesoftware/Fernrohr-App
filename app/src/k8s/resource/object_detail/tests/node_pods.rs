//! #186: a Node's own live pods region. It reuses the Pods panel's table and
//! shared search (`ui::list_search` #189) rather than duplicating them, over
//! a fake cluster (`k8s::test_cluster`) the test writes to while the panel is
//! open - the same harness shape as `tests::live`'s. Real keystrokes
//! throughout (`simulate_keystrokes`), never a literal `cmd-`.
//!
//! Enter, `d` and `l` opening the right pod *in the window* - dock panels and
//! contexts - are `util::shell::node_pods_window_tests`' instead: this
//! module's bare `Root` has no dock to open one in.

use super::fixtures::{nodes, target};
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_detail::ObjectDetailPanel;
use crate::k8s::resource::pods::PodsPanel;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

const CONTEXT: &str = "node-pods-dev";

fn pod(namespace: &str, uid: &str, name: &str, node: &str) -> Value {
    json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": { "name": name, "namespace": namespace, "uid": uid },
        "spec": { "nodeName": node, "containers": [{ "name": "app", "image": "nginx" }] },
    })
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<ObjectDetailPanel>,
    vcx: VisualTestContext,
}

/// A Node detail panel over `node`, with the registry's real bindings for
/// this panel and the embedded Pods table (`d`, `l`, `/`, Escape), so real
/// keystrokes resolve as they would in the app.
fn open(cx: &mut TestAppContext, node: &str, initial_pods: &[Value]) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_detail::register_commands(&mut registry);
        crate::k8s::resource::pods::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "nodes",
        json!({ "apiVersion": "v1", "kind": "Node", "metadata": { "name": node } }),
    );
    for pod in initial_pods {
        cluster.apply("/api/v1", "pods", pod.clone());
    }
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let target = target(nodes(), None, node);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Object(target.clone()), CONTEXT.into());
        let panel = cx.new(|cx| ObjectDetailPanel::new(target.clone(), scope, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let vcx = VisualTestContext::from_window(window.into(), cx);
    Harness {
        cluster,
        panel,
        vcx,
    }
}

impl Harness {
    /// The Node's embedded pods region - present for every window this
    /// module opens, which always names a Node target.
    fn pods(&mut self) -> Entity<PodsPanel> {
        self.vcx
            .update(|_, cx| self.panel.read(cx).test_node_pods())
            .expect("a Node's pods region")
    }

    /// Runs the app until `done` holds for the region's row names, panicking
    /// with `what` if it never does.
    fn wait_for(&mut self, what: &str, done: impl Fn(&[String]) -> bool) {
        let pods = self.pods();
        for _ in 0..400 {
            self.vcx.run_until_parked();
            self.vcx.update(|window, cx| {
                window.render_frame(cx);
            });
            let names = self.vcx.update(|_, cx| pods.read(cx).test_row_names(cx));
            if done(&names) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let watch_count = self
            .vcx
            .update(|_, cx| pods.read(cx).test_watch_pod_count(cx));
        panic!("never {what} (watch has {watch_count} pods)");
    }

    fn row_names(&mut self) -> Vec<String> {
        let pods = self.pods();
        self.vcx.update(|_, cx| pods.read(cx).test_row_names(cx))
    }

    /// Focuses the embedded pods region directly - what Tab reaching it (its
    /// own, separately covered test) lands on.
    fn focus_pods(&mut self) {
        let pods = self.pods();
        self.vcx.update(|window, cx| {
            pods.read(cx).focus_handle(cx).focus(window, cx);
        });
        self.vcx.run_until_parked();
    }

    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }
}

/// A Node's region lists only the pods `spec.nodeName` names, across every
/// namespace - not another node's.
#[gpui_kit::test]
async fn lists_only_this_nodes_pods_across_namespaces(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        "node-a",
        &[
            pod("shop", "u1", "web-1", "node-a"),
            pod("kube-system", "u2", "coredns-1", "node-a"),
            pod("shop", "u3", "elsewhere", "node-b"),
        ],
    );
    harness.wait_for("listed both of node-a's pods", |names| names.len() == 2);

    let mut names = harness.row_names();
    names.sort();
    assert_eq!(names, ["coredns-1", "web-1"]);
}

/// A pod scheduled onto the node later appears live; one deleted disappears -
/// the shared, all-namespaces pods watch every subscriber follows.
#[gpui_kit::test]
async fn a_scheduled_pod_appears_live_and_a_deleted_one_disappears(cx: &mut TestAppContext) {
    let mut harness = open(cx, "node-a", &[pod("shop", "u1", "web-1", "node-a")]);
    harness.wait_for("listed web-1", |names| names == ["web-1"]);

    harness
        .cluster
        .apply("/api/v1", "pods", pod("shop", "u2", "web-2", "node-a"));
    harness.wait_for("listed both pods", |names| names.len() == 2);

    harness.cluster.delete("/api/v1", "pods", "shop", "web-1");
    harness.wait_for("dropped the deleted pod", |names| names == ["web-2"]);
}

/// `/` plus typing narrows the region to matching pods; Escape clears it.
#[gpui_kit::test]
async fn slash_filters_and_escape_clears(cx: &mut TestAppContext) {
    let mut harness = open(
        cx,
        "node-a",
        &[
            pod("shop", "u1", "web-1", "node-a"),
            pod("shop", "u2", "api-1", "node-a"),
        ],
    );
    harness.wait_for("listed both pods", |names| names.len() == 2);
    harness.focus_pods();

    harness.press("/");
    harness.vcx.simulate_input("web");
    harness.vcx.run_until_parked();
    assert_eq!(harness.row_names(), ["web-1"], "narrowed to the match");

    harness.press("escape");
    let mut names = harness.row_names();
    names.sort();
    assert_eq!(names, ["api-1", "web-1"], "escape restored every row");
}

/// With no pods on it, the region says so rather than showing an empty table
/// with nothing to explain it.
#[gpui_kit::test]
async fn no_pods_on_the_node_shows_an_explicit_message(cx: &mut TestAppContext) {
    let mut harness = open(cx, "node-a", &[pod("shop", "u1", "elsewhere", "node-b")]);
    harness.vcx.run_until_parked();
    for _ in 0..200 {
        harness.vcx.run_until_parked();
        harness.vcx.update(|window, cx| window.render_frame(cx));
        if harness.vcx.debug_bounds("node-pods-empty").is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never showed the no-pods message");
}

/// A Connecting (not yet live) cluster shows the region's own connecting
/// state rather than panicking - the embedded panel's own, over the same
/// shared connection.
#[gpui_kit::test]
async fn a_connecting_cluster_shows_a_sensible_state(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(
            cx,
            "node-pods-connecting",
            ConnectionState::Connecting,
        );
    });
    let target = target(nodes(), None, "node-a");
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(
            NavTarget::Object(target.clone()),
            "node-pods-connecting".into(),
        );
        let panel = cx.new(|cx| ObjectDetailPanel::new(target.clone(), scope, cx));
        Root::new(panel, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    // Renders without panicking over a connection that never completes.
    vcx.update(|window, cx| window.render_frame(cx));
}

/// Tab, from the detail panel, reaches the embedded pods region - the same
/// focus path every other list panel's table is a stop on.
#[gpui_kit::test]
async fn tab_reaches_the_pods_region(cx: &mut TestAppContext) {
    let mut harness = open(cx, "node-a", &[pod("shop", "u1", "web-1", "node-a")]);
    harness.wait_for("listed web-1", |names| names == ["web-1"]);
    harness.vcx.update(|window, cx| {
        harness.panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    harness.vcx.run_until_parked();

    let pods = harness.pods();
    let mut reached = false;
    for _ in 0..40 {
        harness.vcx.simulate_keystrokes("tab");
        harness.vcx.run_until_parked();
        let focused = harness
            .vcx
            .update(|window, cx| pods.read(cx).focus_handle(cx).contains_focused(window, cx));
        if focused {
            reached = true;
            break;
        }
    }
    assert!(reached, "Tab never reached the embedded pods table");
}

/// A non-Node object's detail has no pods region at all.
#[gpui_kit::test]
async fn a_non_node_objects_detail_has_no_pods_region(cx: &mut TestAppContext) {
    use super::fixtures::deployments;

    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "node-pods-ns", ConnectionState::Connecting);
    });
    let target = target(deployments(), Some("staging"), "web");
    let window = cx.add_window(|_, cx| {
        let scope = PanelScope::new(NavTarget::Object(target.clone()), "node-pods-ns".into());
        ObjectDetailPanel::new(target.clone(), scope, cx)
    });
    let panel = window.root(cx).unwrap();
    assert!(
        cx.update(|cx| panel.read(cx).test_node_pods()).is_none(),
        "a Deployment's detail has no pods region"
    );
}
