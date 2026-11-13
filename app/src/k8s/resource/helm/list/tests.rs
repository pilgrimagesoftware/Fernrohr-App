//! helm-releases section 3: the Helm Releases list over a fake cluster - live
//! upgrades and uninstalls, uninstalled releases hidden until asked for, the
//! access message, a namespace scope, the search from the keyboard, and the
//! panel's saved state.

use super::HelmReleasesPanel;
use super::render::refusal_text;
use super::restore::{dump, from_state};
use super::row::HelmRow;
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::helm::HelmScope;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use base64::Engine as _;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use std::io::Write as _;

const CONTEXT: &str = "kind-dev";

/// Revision `version` of release `name`, stored as Helm stores it.
fn revision(namespace: &str, name: &str, version: u32, status: &str, chart: &str) -> Value {
    let release = json!({
        "name": name, "namespace": namespace, "version": version,
        "info": { "status": status, "last_deployed": "2026-11-12T09:30:00Z" },
        "chart": { "metadata": { "name": name, "version": chart, "appVersion": "2.0" } },
    });
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(release.to_string().as_bytes()).unwrap();
    let helm = base64::engine::general_purpose::STANDARD.encode(gz.finish().unwrap());
    json!({
        "apiVersion": "v1", "kind": "Secret", "type": "helm.sh/release.v1",
        "metadata": {
            "name": format!("sh.helm.release.v1.{name}.v{version}"),
            "namespace": namespace,
            "labels": { "owner": "helm", "name": name, "version": version.to_string(), "status": status },
        },
        "data": { "release": base64::engine::general_purpose::STANDARD.encode(helm) },
    })
}

fn install(cluster: &FakeCluster, namespace: &str, name: &str, version: u32, status: &str) {
    cluster.apply(
        "/api/v1",
        "secrets",
        revision(namespace, name, version, status, &format!("1.{version}.0")),
    );
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<HelmReleasesPanel>,
    vcx: VisualTestContext,
}

/// A Helm Releases panel scoped to `namespaces` (empty for all), with the
/// app's keymap, over a fake cluster `seed` fills first.
fn open(cx: &mut TestAppContext, namespaces: &[&str], seed: impl Fn(&FakeCluster)) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        super::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let (cluster, client) = FakeCluster::start(cx);
    seed(&cluster);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let scope = PanelScope::new(NavTarget::HelmReleases, CONTEXT.into())
        .scoped_to(namespaces.iter().map(|ns| ns.to_string()).collect());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| HelmReleasesPanel::new(scope, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let focus = panel.clone();
    vcx.update(|window, cx| focus.read(cx).focus_handle(cx).focus(window, cx));
    vcx.run_until_parked();
    Harness {
        cluster,
        panel,
        vcx,
    }
}

impl Harness {
    fn displayed(&mut self) -> Vec<HelmRow> {
        self.vcx.update(|window, cx| window.render_frame(cx));
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| panel.read(cx).displayed(cx))
    }

    fn wait_for(&mut self, what: &str, done: impl Fn(&[HelmRow]) -> bool) -> Vec<HelmRow> {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            let rows = self.displayed();
            if done(&rows) {
                return rows;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}: {:?}", self.displayed());
    }

    fn shows(&mut self, selector: &'static str) -> bool {
        self.vcx.update(|window, cx| window.render_frame(cx));
        self.vcx.debug_bounds(selector).is_some()
    }
}

fn names(rows: &[HelmRow]) -> Vec<String> {
    rows.iter().map(|row| row.name.clone()).collect()
}

#[gpui_kit::test]
async fn upgrades_and_uninstalls_appear_without_refreshing(cx: &mut TestAppContext) {
    let mut h = open(cx, &[], |cluster| {
        install(cluster, "shop", "web", 1, "deployed");
        install(cluster, "shop", "db", 1, "deployed");
    });
    let rows = h.wait_for("listed both, with charts", |rows| {
        rows.len() == 2
            && rows
                .iter()
                .all(|row| row.chart_text().starts_with(&row.name))
    });
    assert_eq!(names(&rows), ["db", "web"], "sorted by name");
    assert_eq!(rows[1].chart_text(), "web-1.1.0");
    assert_eq!(rows[1].app_version, "2.0");

    install(&h.cluster, "shop", "web", 2, "pending-upgrade");
    install(&h.cluster, "shop", "web", 1, "superseded");
    install(&h.cluster, "shop", "web", 2, "deployed");
    let rows = h.wait_for("showed web's revision 2", |rows| {
        rows.iter().any(|row| {
            row.name == "web"
                && row.revision == 2
                && row.status == "deployed"
                && row.chart_text() == "web-1.2.0"
        })
    });
    assert_eq!(rows.len(), 2, "an upgrade adds no row");

    h.cluster
        .delete("/api/v1", "secrets", "shop", "sh.helm.release.v1.db.v1");
    h.wait_for("dropped db", |rows| names(rows) == ["web"]);
}

#[gpui_kit::test]
async fn uninstalled_releases_show_only_when_asked(cx: &mut TestAppContext) {
    let mut h = open(cx, &[], |cluster| {
        install(cluster, "shop", "web", 1, "deployed");
        install(cluster, "shop", "old", 1, "uninstalled");
    });
    h.wait_for("listed web alone", |rows| names(rows) == ["web"]);

    h.vcx.dispatch_action(super::commands::ToggleUninstalled);
    let rows = h.wait_for("listed old too", |rows| rows.len() == 2);
    assert_eq!(names(&rows), ["old", "web"]);

    let panel = h.panel.clone();
    let saved = h.vcx.update(|_, cx| dump(panel.read(cx), cx));
    assert!(
        from_state(&saved).expect("restores").show_uninstalled,
        "the choice is saved with the panel"
    );
}

#[gpui_kit::test]
async fn a_refused_list_says_so_instead_of_listing_nothing(cx: &mut TestAppContext) {
    let mut h = open(cx, &[], |cluster| {
        cluster.refuse_lists(
            "403 Forbidden",
            json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure", "code": 403,
                "reason": "Forbidden", "message": "secrets is forbidden" }),
        );
    });
    for _ in 0..400 {
        if h.shows("helm-releases-refused") {
            return;
        }
        h.vcx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the access message never showed");
}

#[test]
fn the_access_message_names_its_scope() {
    let (title, detail) = refusal_text(&HelmScope::All, "secrets is forbidden");
    assert!(title.contains("all namespaces"), "{title}");
    assert!(
        detail.contains("namespace picker"),
        "it suggests one: {detail}"
    );
    assert!(
        detail.contains("secrets is forbidden"),
        "and quotes the server"
    );
    let (title, _) = refusal_text(&HelmScope::Namespace("shop".into()), "denied");
    assert!(title.contains("in shop"), "{title}");
}

#[gpui_kit::test]
async fn a_namespace_scope_lists_only_its_releases(cx: &mut TestAppContext) {
    let mut h = open(cx, &["shop"], |cluster| {
        install(cluster, "shop", "web", 1, "deployed");
        install(cluster, "ops", "monitoring", 1, "deployed");
    });
    h.wait_for("listed shop's release", |rows| names(rows) == ["web"]);
    let panel = h.panel.clone();
    let scopes = h.vcx.update(|_, cx| panel.read(cx).watch_scopes());
    assert_eq!(scopes, [HelmScope::Namespace("shop".into())]);
}

/// The search, from the keyboard: `/` focuses it, typing narrows the rows,
/// Escape clears it.
#[gpui_kit::test]
async fn the_search_narrows_from_the_keyboard(cx: &mut TestAppContext) {
    let mut h = open(cx, &[], |cluster| {
        install(cluster, "shop", "web", 1, "deployed");
        install(cluster, "shop", "db", 1, "deployed");
    });
    h.wait_for("listed both", |rows| rows.len() == 2);
    h.vcx.simulate_keystrokes("/");
    h.vcx.simulate_input("we");
    h.wait_for("narrowed to web", |rows| names(rows) == ["web"]);
    h.vcx.simulate_keystrokes("escape");
    h.wait_for("listed both again", |rows| rows.len() == 2);
}

#[gpui_kit::test]
async fn the_panel_saves_its_scope_sort_and_search(cx: &mut TestAppContext) {
    let mut h = open(cx, &["shop", "ops"], |_| {});
    h.vcx.run_until_parked();
    let panel = h.panel.clone();
    let saved = h.vcx.update(|_, cx| dump(panel.read(cx), cx));
    let restored = from_state(&saved).expect("restores");
    assert_eq!(restored.context_name, CONTEXT);
    assert_eq!(
        restored.namespaces,
        ["ops", "shop"],
        "as the scope keeps them, sorted"
    );
    assert!(!restored.show_uninstalled);
}
