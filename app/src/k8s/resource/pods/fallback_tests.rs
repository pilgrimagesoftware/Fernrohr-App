//! #457: a Pods panel whose all-namespaces list is refused lists the namespaces it is
//! scoped to one by one, as one table, against a fake cluster that only allows lists in
//! some namespaces.

use super::PodsPanel;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::json;

const CONTEXT: &str = "rbac-ctx";

struct Fixture {
    panel: Entity<PodsPanel>,
    vcx: VisualTestContext,
}

/// A Pods panel scoped to `namespaces` on a cluster with one pod in each of `staging`,
/// `prod` and `locked`, where the account may list only in `staging` and `prod`.
fn open(cx: &mut TestAppContext, namespaces: &[&str]) -> Fixture {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (cluster, client) = FakeCluster::start(cx);
    for (name, namespace) in [("web-1", "staging"), ("api-1", "prod"), ("db-1", "locked")] {
        cluster.apply(
            "/api/v1",
            "pods",
            json!({ "apiVersion": "v1", "kind": "Pod",
                "metadata": { "name": name, "namespace": namespace, "uid": format!("uid-{name}") },
                "status": { "phase": "Running" } }),
        );
    }
    cluster.allow_lists_only_in(&["staging", "prod"]);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let scope = PanelScope::new(NavTarget::pods(), CONTEXT.into())
        .scoped_to(namespaces.iter().map(|ns| ns.to_string()).collect());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| PodsPanel::new(scope, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    Fixture {
        panel: built.expect("the window built its panel"),
        vcx: VisualTestContext::from_window(window.into(), cx),
    }
}

impl Fixture {
    /// The rows drawn now, sorted.
    fn rows(&mut self) -> Vec<String> {
        self.vcx.update(|window, cx| window.render_frame(cx));
        let panel = self.panel.clone();
        let mut rows = self.vcx.update(|_, cx| panel.read(cx).test_row_names(cx));
        rows.sort();
        rows
    }

    fn refusal(&mut self) -> Option<String> {
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| panel.read(cx).listed_refusal(cx))
    }

    fn set_namespaces(&mut self, namespaces: &[&str]) {
        let panel = self.panel.clone();
        let namespaces: Vec<String> = namespaces.iter().map(|ns| ns.to_string()).collect();
        self.vcx.update(|_, cx| {
            panel.update(cx, |panel, cx| panel.set_namespaces(namespaces, cx));
        });
    }

    fn wait_for(&mut self, what: &str, done: impl Fn(&mut Self) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if done(self) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}: drawing {:?}", self.rows());
    }

    fn subscribers(&mut self, namespace: &str) -> usize {
        let key = WatchKey::PodsIn(namespace.to_string());
        self.vcx
            .update(|_, cx| ClusterRegistry::subscribers(cx, CONTEXT, &key))
    }
}

#[gpui_kit::test]
async fn a_refused_pods_list_shows_its_namespaces_as_one_table(cx: &mut TestAppContext) {
    let mut f = open(cx, &["staging", "prod"]);
    f.wait_for("drew both namespaces", |f| f.rows().len() == 2);

    assert_eq!(f.rows(), ["api-1", "web-1"]);
    assert_eq!(f.refusal(), None);
    let panel = f.panel.clone();
    let mut watched = f.vcx.update(|_, cx| panel.read(cx).fallback.namespaces());
    watched.sort();
    assert_eq!(watched, ["prod", "staging"]);
}

#[gpui_kit::test]
async fn narrowing_the_scope_releases_a_namespace(cx: &mut TestAppContext) {
    let mut f = open(cx, &["staging", "prod"]);
    f.wait_for("drew both namespaces", |f| f.rows().len() == 2);

    f.set_namespaces(&["prod"]);
    f.wait_for("drew prod alone", |f| f.rows() == ["api-1"]);

    assert_eq!(f.subscribers("staging"), 0, "staging's watch released");
    assert_eq!(f.subscribers("prod"), 1);
}

#[gpui_kit::test]
async fn a_namespace_the_account_cant_read_leaves_the_others(cx: &mut TestAppContext) {
    let mut f = open(cx, &["staging", "locked"]);
    f.wait_for("drew staging", |f| f.rows() == ["web-1"]);

    assert_eq!(f.refusal(), None);
}

#[gpui_kit::test]
async fn scoped_to_every_namespace_it_says_to_choose_some(cx: &mut TestAppContext) {
    let mut f = open(cx, &[]);
    f.wait_for("refused", |f| f.refusal().is_some());
    let panel = f.panel.clone();
    let message = f.vcx.update(|_, cx| panel.read(cx).refusal_message());
    assert!(message.contains("Choose the namespaces"), "{message}");

    f.set_namespaces(&["staging"]);
    f.wait_for("drew staging", |f| f.rows() == ["web-1"]);
}

/// The quick look reads the selected pod from its own namespace's table.
#[gpui_kit::test]
async fn a_pod_is_found_in_its_namespaces_table(cx: &mut TestAppContext) {
    let mut f = open(cx, &["staging", "prod"]);
    f.wait_for("drew both namespaces", |f| f.rows().len() == 2);
    let panel = f.panel.clone();
    let found = f.vcx.update(|_, cx| {
        let panel = panel.read(cx);
        (
            panel.find_listed("prod", "api-1", cx).is_some(),
            panel.find_listed("staging", "web-1", cx).is_some(),
            panel.find_listed("locked", "db-1", cx).is_some(),
        )
    });
    assert_eq!(found, (true, true, false));
}
