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
            "notes": "Visit the app at http://app.example.com\nThen run: kubectl get pods",
        },
        "chart": { "metadata": { "name": name, "version": format!("1.{version}.0"), "appVersion": "2.0" } },
        "config": { "replicaCount": version, "image": { "tag": "2.0" } },
        "manifest": "---\napiVersion: v1\nkind: Secret\nmetadata:\n  name: web\nstringData:\n  password: hunter2\n---\napiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: web\n",
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
        Some("Visit the app at http://app.example.com\nThen run: kubectl get pods".to_string())
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

/// Notes are the chart author's text: a fence and a link in them show as
/// written, line by line, never as Markdown.
#[test]
fn notes_show_as_written() {
    let notes =
        "Get started:\n```\nkubectl get pods\n```\n\nSee [the docs](http://docs.example.com)";
    let lines = super::render::plain_lines(notes);
    assert_eq!(
        lines,
        [
            "Get started:",
            "```",
            "kubectl get pods",
            "```",
            " ",
            "See [the docs](http://docs.example.com)",
        ]
    );
}

fn shows(h: &mut Harness, selector: &'static str) -> bool {
    use gpui_kit::test::TestWindowExt as _;
    h.vcx.update(|window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector).is_some()
}

/// helm-releases 5.1: Values opens hidden; `r` reveals the selected (deployed)
/// revision's values and `r` again hides them.
#[gpui_kit::test]
async fn values_open_hidden_and_reveal_by_key(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.wait_for("listed", |panel, cx| panel.history(cx).len() == 3);
    h.vcx.simulate_keystrokes("3");
    h.vcx.run_until_parked();
    assert_eq!(h.read(|panel, _| panel.tab), ReleaseTab::Values);
    assert!(
        shows(&mut h, "helm-release-hidden"),
        "hidden until revealed"
    );
    assert!(h.read(|panel, cx| panel.revealed_text(ReleaseTab::Values, cx).is_none()));

    h.vcx.simulate_keystrokes("r");
    h.wait_for("revealed the values", |panel, cx| {
        panel.revealed_text(ReleaseTab::Values, cx).is_some()
    });
    let values = h.read(|panel, cx| {
        panel
            .revealed_text(ReleaseTab::Values, cx)
            .map(str::to_string)
    });
    assert!(
        values.unwrap().contains("replicaCount: 3"),
        "the deployed revision's"
    );

    h.vcx.simulate_keystrokes("r");
    h.vcx.run_until_parked();
    assert!(
        h.read(|panel, cx| panel.revealed_text(ReleaseTab::Values, cx).is_none()),
        "hidden again"
    );
}

/// helm-releases 5.2: a revealed manifest shows its Secret's keys and sizes,
/// never the value, and every other document as written.
#[gpui_kit::test]
async fn a_revealed_manifest_redacts_its_secrets(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.wait_for("listed", |panel, cx| panel.history(cx).len() == 3);
    h.vcx.simulate_keystrokes("4");
    h.vcx.simulate_keystrokes("r");
    h.wait_for("revealed the manifest", |panel, cx| {
        panel.revealed_text(ReleaseTab::Manifest, cx).is_some()
    });
    let manifest = h
        .read(|panel, cx| {
            panel
                .revealed_text(ReleaseTab::Manifest, cx)
                .map(str::to_string)
        })
        .unwrap();
    assert!(!manifest.contains("hunter2"), "{manifest}");
    assert!(manifest.contains("redacted: 7 bytes"), "{manifest}");
    assert!(manifest.contains("kind: ConfigMap"), "{manifest}");
}

/// From History, `v` shows the selected revision's values, saying which; the
/// reveal belongs to one tab and revision, so changing either drops it.
#[gpui_kit::test]
async fn an_earlier_revisions_values_from_history(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.wait_for("listed", |panel, cx| panel.history(cx).len() == 3);
    h.vcx.simulate_keystrokes("3");
    h.vcx.simulate_keystrokes("r");
    h.wait_for("revealed", |panel, cx| {
        panel.revealed_text(ReleaseTab::Values, cx).is_some()
    });

    h.vcx.simulate_keystrokes("2");
    h.vcx.run_until_parked();
    assert!(
        h.read(|panel, _| panel.revealed.is_none()),
        "a tab change drops the reveal"
    );
    h.vcx.simulate_keystrokes("down");
    h.vcx.simulate_keystrokes("v");
    h.vcx.run_until_parked();
    assert_eq!(h.read(|panel, _| panel.tab), ReleaseTab::Values);
    assert_eq!(h.read(|panel, cx| panel.selected_revision(cx)), Some(2));
    assert!(shows(&mut h, "helm-release-hidden"), "still hidden");

    h.vcx.simulate_keystrokes("r");
    h.wait_for("revealed revision 2", |panel, cx| {
        panel
            .revealed_text(ReleaseTab::Values, cx)
            .is_some_and(|values| values.contains("replicaCount: 2"))
    });
}

/// Copy is offered only while the text is revealed, and copies it.
#[gpui_kit::test]
async fn copy_is_only_offered_while_revealed(cx: &mut TestAppContext) {
    use super::commands::CopyValues;
    let mut h = open(cx);
    h.wait_for("listed", |panel, cx| panel.history(cx).len() == 3);
    h.vcx.simulate_keystrokes("3");
    h.vcx.run_until_parked();
    let available = |h: &mut Harness| {
        use gpui_kit::test::TestWindowExt as _;
        h.vcx.update(|window, cx| {
            window.render_frame(cx);
            window.is_action_available(&CopyValues, cx)
        })
    };
    assert!(!available(&mut h), "hidden: nothing to copy");
    h.vcx.simulate_keystrokes("r");
    h.wait_for("revealed", |panel, cx| {
        panel.revealed_text(ReleaseTab::Values, cx).is_some()
    });
    assert!(available(&mut h), "revealed: Copy Values is offered");
    h.vcx.dispatch_action(CopyValues);
    let copied = h
        .vcx
        .update(|_, cx| cx.read_from_clipboard())
        .and_then(|item| item.text());
    assert!(copied.is_some_and(|text| text.contains("replicaCount: 3")));
}

/// Drags across the element `selector` names and returns the selected text.
fn drag_select(h: &mut Harness, selector: &'static str) -> String {
    use gpui_kit::{Modifiers, MouseButton, point, px};
    h.vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let bounds = h.vcx.debug_bounds(selector).expect("the text is drawn");
    let start = point(bounds.left() + px(1.), bounds.top() + px(2.));
    let end = point(bounds.right() - px(1.), bounds.bottom() - px(1.));
    h.vcx
        .simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    h.vcx
        .simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    h.vcx
        .simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    h.vcx.run_until_parked();
    h.vcx.update(|window, cx| {
        let _ = window.draw(cx);
        gpui_kit::base::TextSelection::selected_text(window, cx)
    })
}

/// Notes often hold commands to copy: their lines drag-select like log lines.
#[gpui_kit::test]
async fn notes_can_be_selected(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.vcx.simulate_keystrokes("5");
    h.wait_for("read the notes", |panel, cx| panel.notes(cx).is_some());
    let selected = drag_select(&mut h, "helm-release-notes");
    let (first, second) = (selected.find("Visit the app"), selected.find("Then run"));
    assert!(
        matches!((first, second), (Some(a), Some(b)) if a < b),
        "both lines, in order: {selected:?}"
    );
}

/// Revealed values drag-select the same way.
#[gpui_kit::test]
async fn revealed_values_can_be_selected(cx: &mut TestAppContext) {
    let mut h = open(cx);
    h.wait_for("listed", |panel, cx| panel.history(cx).len() == 3);
    h.vcx.simulate_keystrokes("3");
    h.vcx.simulate_keystrokes("r");
    h.wait_for("revealed", |panel, cx| {
        panel.revealed_text(ReleaseTab::Values, cx).is_some()
    });
    let selected = drag_select(&mut h, "helm-release-text");
    let (first, second) = (selected.find("replicaCount: 3"), selected.find("image:"));
    assert!(
        matches!((first, second), (Some(a), Some(b)) if a < b),
        "both lines, in order: {selected:?}"
    );
}
