//! #457: a list panel whose cluster-wide list is refused lists the namespaces it is
//! scoped to one by one, as one list, against a cluster that only allows lists in some
//! namespaces.

use super::combined_phase;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::{ClusterRegistry, KindKey, WatchKey};
use crate::k8s::resource::load_phase::LoadPhase;
use crate::k8s::resource::object_list::ObjectListPanel;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use kube::core::GroupVersionKind;
use serde_json::json;

const CONTEXT: &str = "rbac-ctx";

fn deployments() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("apps", "v1", "Deployment"),
        plural: "deployments".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

struct Fixture {
    panel: Entity<ObjectListPanel>,
    vcx: VisualTestContext,
}

/// A Deployments list scoped to `namespaces` on a cluster with one Deployment in each of
/// `staging`, `prod` and `locked`, where the account may list only in `staging` and `prod`.
fn open(cx: &mut TestAppContext, namespaces: &[&str]) -> Fixture {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let (cluster, client) = FakeCluster::start(cx);
    for (name, namespace) in [("web", "staging"), ("api", "prod"), ("db", "locked")] {
        cluster.apply(
            "/apis/apps/v1",
            "deployments",
            json!({ "apiVersion": "apps/v1", "kind": "Deployment",
                "metadata": { "name": name, "namespace": namespace, "uid": format!("uid-{name}") } }),
        );
    }
    cluster.allow_lists_only_in(&["staging", "prod"]);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let scope = PanelScope::new(NavTarget::Kind(deployments()), CONTEXT.into())
        .scoped_to(namespaces.iter().map(|ns| ns.to_string()).collect());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| ObjectListPanel::new(deployments(), scope, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    Fixture {
        panel: built.expect("the window built its panel"),
        vcx: VisualTestContext::from_window(window.into(), cx),
    }
}

impl Fixture {
    /// The names the panel lists now, sorted.
    fn names(&mut self) -> Vec<String> {
        let panel = self.panel.clone();
        let mut names: Vec<String> = self.vcx.update(|_, cx| {
            panel
                .read(cx)
                .visible_rows(cx)
                .into_iter()
                .map(|row| row.object.name.clone())
                .collect()
        });
        names.sort();
        names
    }

    fn fallback(&mut self) -> Vec<String> {
        let panel = self.panel.clone();
        self.vcx
            .update(|_, cx| panel.read(cx).fallback.namespaces())
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

    /// Polls until `done` holds, panicking with `what` if it never does.
    fn wait_for(&mut self, what: &str, done: impl Fn(&mut Self) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if done(self) {
                self.vcx.update(|window, cx| window.render_frame(cx));
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}: listing {:?}", self.names());
    }

    fn subscribers(&mut self, namespace: &str) -> usize {
        let key = WatchKey::Kind(KindKey {
            kind: deployments(),
            namespace: Some(namespace.to_string()),
        });
        self.vcx
            .update(|_, cx| ClusterRegistry::subscribers(cx, CONTEXT, &key))
    }
}

#[gpui_kit::test]
async fn a_refused_list_shows_its_namespaces_as_one_list(cx: &mut TestAppContext) {
    let mut f = open(cx, &["staging", "prod"]);
    f.wait_for("listed both namespaces", |f| f.names().len() == 2);

    assert_eq!(f.names(), ["api", "web"]);
    let scoped = {
        let panel = f.panel.clone();
        f.vcx
            .update(|_, cx| panel.read(cx).scope.namespaces.clone())
    };
    assert_eq!(
        f.fallback(),
        scoped,
        "one watch per namespace, in the scope's order"
    );
    assert_eq!(f.refusal(), None, "the rows, not the refusal");
}

#[gpui_kit::test]
async fn narrowing_the_scope_releases_a_namespace(cx: &mut TestAppContext) {
    let mut f = open(cx, &["staging", "prod"]);
    f.wait_for("listed both namespaces", |f| f.names().len() == 2);

    f.set_namespaces(&["staging"]);
    f.wait_for("listed staging alone", |f| f.names() == ["web"]);

    assert_eq!(f.fallback(), ["staging"]);
    assert_eq!(f.subscribers("prod"), 0, "prod's watch released");
    assert_eq!(f.subscribers("staging"), 1);
}

#[gpui_kit::test]
async fn a_namespace_the_account_cant_read_leaves_the_others(cx: &mut TestAppContext) {
    let mut f = open(cx, &["staging", "locked"]);
    f.wait_for("listed staging", |f| f.names() == ["web"]);

    assert_eq!(f.refusal(), None, "staging still lists");
}

#[gpui_kit::test]
async fn every_namespace_refused_shows_the_refusal(cx: &mut TestAppContext) {
    let mut f = open(cx, &["locked"]);
    f.wait_for("refused", |f| f.refusal().is_some());

    assert!(f.names().is_empty());
}

#[gpui_kit::test]
async fn scoped_to_every_namespace_it_says_to_choose_some(cx: &mut TestAppContext) {
    let mut f = open(cx, &[]);
    f.wait_for("refused", |f| f.refusal().is_some());

    assert!(f.fallback().is_empty(), "nothing to fall back to");
    let panel = f.panel.clone();
    let message = f.vcx.update(|_, cx| panel.read(cx).refusal_message());
    assert!(message.contains("Choose the namespaces"), "{message}");

    f.set_namespaces(&["prod"]);
    f.wait_for("listed prod", |f| f.names() == ["api"]);
}

#[test]
fn phases_combine_as_one_list() {
    use LoadPhase::*;
    assert_eq!(combined_phase([Loaded, Loaded]), Loaded);
    assert_eq!(
        combined_phase([FirstLoad { received: 2 }, Loaded, FirstLoad { received: 1 }]),
        FirstLoad { received: 3 },
        "loading until every namespace has listed"
    );
    assert_eq!(
        combined_phase([Loaded, Refreshing { received: 4 }]),
        Refreshing { received: 4 }
    );
    assert_eq!(
        combined_phase([Refreshing { received: 1 }, FirstLoad { received: 0 }]),
        FirstLoad { received: 0 }
    );
}
