//! helm-releases section 4: the release detail panel over a fake cluster -
//! the Overview, History and Notes tabs by their keys, a live upgrade, the
//! release going away, and the panel's saved state.

use super::HelmReleasePanel;
use super::panel::ReleaseTab;
use super::restore::{dump, from_state};
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::helm::ReleaseKey;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use base64::Engine as _;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use std::io::Write as _;

const CONTEXT: &str = "kind-dev";

fn revision(name: &str, version: u32, status: &str) -> Value {
    let release = json!({
        "name": name, "namespace": "shop", "version": version,
        "info": {
            "status": status,
            "first_deployed": "2026-11-01T10:00:00Z",
            "last_deployed": format!("2026-11-1{version}T09:00:00Z"),
            "description": format!("Upgrade {version} complete"),
            "notes": "Visit the app at http://app.example.com",
        },
        "chart": { "metadata": { "name": name, "version": format!("1.{version}.0"), "appVersion": "2.0" } },
    });
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(release.to_string().as_bytes()).unwrap();
    let helm = base64::engine::general_purpose::STANDARD.encode(gz.finish().unwrap());
    json!({
        "apiVersion": "v1", "kind": "Secret", "type": "helm.sh/release.v1",
        "metadata": {
            "name": format!("sh.helm.release.v1.{name}.v{version}"),
            "namespace": "shop",
            "labels": { "owner": "helm", "name": name, "version": version.to_string(), "status": status },
        },
        "data": { "release": base64::engine::general_purpose::STANDARD.encode(helm) },
    })
}

fn put(cluster: &FakeCluster, version: u32, status: &str) {
    cluster.apply("/api/v1", "secrets", revision("web", version, status));
}

fn key() -> ReleaseKey {
    ReleaseKey {
        namespace: "shop".into(),
        name: "web".into(),
    }
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<HelmReleasePanel>,
    vcx: VisualTestContext,
}

/// `web` at revisions 1-3 (3 deployed), its detail panel open and focused,
/// with the app's keymap.
fn open(cx: &mut TestAppContext) -> Harness {
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
    put(&cluster, 1, "superseded");
    put(&cluster, 2, "superseded");
    put(&cluster, 3, "deployed");
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let scope = PanelScope::new(NavTarget::HelmRelease(key()), CONTEXT.into());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| HelmReleasePanel::new(key(), scope, cx));
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
    fn wait_for(&mut self, what: &str, done: impl Fn(&HelmReleasePanel, &gpui_kit::App) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            let panel = self.panel.clone();
            if self.vcx.update(|_, cx| done(panel.read(cx), cx)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }

    fn read<R>(&mut self, read: impl Fn(&HelmReleasePanel, &gpui_kit::App) -> R) -> R {
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| read(panel.read(cx), cx))
    }
}

#[gpui_kit::test]
async fn the_overview_shows_the_deployed_release(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.wait_for("read revision 3", |panel, _| panel.detail(3).is_some());
    assert_eq!(h.read(|panel, _| panel.tab), ReleaseTab::Overview);
    let detail = h.read(|panel, _| panel.detail(3).cloned()).unwrap();
    assert_eq!(detail.chart.version, "1.3.0");
    assert_eq!(detail.chart.app_version.as_deref(), Some("2.0"));
    assert_eq!(
        detail.info.first_deployed.as_deref(),
        Some("2026-11-01T10:00:00Z")
    );
    assert_eq!(
        detail.info.description.as_deref(),
        Some("Upgrade 3 complete")
    );
    assert!(
        h.read(|panel, _| panel.detail(1).is_none()),
        "earlier revisions wait for History"
    );
}

/// History by its key: every revision newest first, each read, the deployed
/// one selected; Down moves the selection to the one before.
#[gpui_kit::test]
async fn history_lists_every_revision_by_its_key(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.wait_for("listed the history", |panel, cx| {
        panel.history(cx).len() == 3
    });
    h.vcx.simulate_keystrokes("2");
    h.wait_for("read every revision", |panel, _| {
        (1..=3).all(|version| panel.detail(version).is_some())
    });
    assert_eq!(h.read(|panel, _| panel.tab), ReleaseTab::History);
    let versions: Vec<u32> = h.read(|panel, cx| {
        panel
            .history(cx)
            .iter()
            .map(|revision| revision.version)
            .collect()
    });
    assert_eq!(versions, [3, 2, 1], "newest first");
    assert_eq!(
        h.read(|panel, cx| panel.selected_revision(cx)),
        Some(3),
        "the deployed one"
    );
    h.vcx.simulate_keystrokes("down");
    h.vcx.run_until_parked();
    assert_eq!(h.read(|panel, cx| panel.selected_revision(cx)), Some(2));
}

#[gpui_kit::test]
async fn notes_show_by_their_key(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.vcx.simulate_keystrokes("5");
    h.wait_for("read the notes", |panel, cx| panel.notes(cx).is_some());
    assert_eq!(h.read(|panel, _| panel.tab), ReleaseTab::Notes);
    assert_eq!(
        h.read(|panel, cx| panel.notes(cx).map(str::to_string)),
        Some("Visit the app at http://app.example.com".to_string())
    );
}

#[gpui_kit::test]
async fn an_upgrade_and_an_uninstall_follow_the_cluster(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.wait_for("read revision 3", |panel, _| panel.detail(3).is_some());
    put(&h.cluster, 4, "pending-upgrade");
    put(&h.cluster, 3, "superseded");
    put(&h.cluster, 4, "deployed");
    h.wait_for("followed the upgrade", |panel, cx| {
        panel.history(cx).first().map(|revision| revision.version) == Some(4)
            && panel
                .detail(4)
                .is_some_and(|detail| detail.chart.version == "1.4.0")
    });

    for version in 1..=4 {
        h.cluster.delete(
            "/api/v1",
            "secrets",
            "shop",
            &format!("sh.helm.release.v1.web.v{version}"),
        );
    }
    h.wait_for("said the release is gone", |panel, cx| panel.gone(cx));
}

#[gpui_kit::test]
async fn the_panel_saves_its_release_and_tab(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.vcx.simulate_keystrokes("2");
    h.vcx.run_until_parked();
    let saved = h.read(|panel, _| dump(panel));
    let (context, release) = from_state(&saved).expect("restores");
    assert_eq!((context.as_str(), release), (CONTEXT, key()));
    assert_eq!(saved["tab"], "history");
}
