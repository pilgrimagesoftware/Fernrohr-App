//! `k9s-remaining-keybindings` section 2 in a window with the panel's keymap,
//! over a fake cluster that records and applies patches: `e` opens the editor
//! over the manifest, Escape drops the edit, `cmd-s` saves - unless the text
//! isn't a manifest for this object, when nothing is sent - and a refused save
//! keeps the edited text.

use super::fixtures::{deployments, kind, target};
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_detail::{ObjectDetailPanel, register_commands};
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::KeymapConfig;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};

const CONTEXT: &str = "kind-dev";
const APPS: &str = "/apis/apps/v1";

fn deployment(replicas: i64) -> Value {
    json!({
        "apiVersion": "apps/v1", "kind": "Deployment",
        "metadata": { "name": "web", "namespace": "staging", "uid": "d1" },
        "spec": { "replicas": replicas, "selector": { "matchLabels": { "app": "web" } }, "template": {} },
    })
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<ObjectDetailPanel>,
    vcx: VisualTestContext,
}

/// The panel on `staging/web` of `kind`, loaded, focused, its keys bound.
fn open(
    cx: &mut TestAppContext,
    kind: DiscoveredKind,
    prefix: &str,
    plural: &str,
    initial: Value,
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
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(prefix, plural, initial);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let target = target(kind, Some("staging"), "web");
    let window = cx.add_window(|_, cx| {
        let scope = PanelScope::new(NavTarget::Object(target.clone()), CONTEXT.into());
        ObjectDetailPanel::new(target, scope, cx)
    });
    let panel = window.root(cx).unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle.clone().focus(window, cx);
    });
    let mut harness = Harness {
        cluster,
        panel,
        vcx,
    };
    harness.wait_for("loaded the object", |panel, _| panel.object().is_some());
    harness
}

impl Harness {
    fn wait_for(&mut self, what: &str, done: impl Fn(&ObjectDetailPanel, &gpui_kit::App) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if self.vcx.update(|_, cx| done(self.panel.read(cx), cx)) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }

    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    fn edit_text(&mut self) -> Option<String> {
        self.vcx.update(|_, cx| self.panel.read(cx).edit_text(cx))
    }

    /// Replaces the editor's text, as typing it would.
    fn type_text(&mut self, text: &str) {
        let editor = self.vcx.update(|_, cx| {
            self.panel
                .read(cx)
                .edit
                .as_ref()
                .expect("editing")
                .editor
                .clone()
        });
        self.vcx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_value(text.to_string(), window, cx)
            });
        });
        self.vcx.run_until_parked();
    }

    fn edit_failure(&mut self) -> Option<String> {
        self.vcx.update(|_, cx| {
            self.panel
                .read(cx)
                .edit
                .as_ref()
                .and_then(|edit| edit.failure.as_ref())
                .map(|failure| failure.message.clone())
        })
    }
}

/// The manifest with `replicas` replicas, as a user's edit would leave it.
fn edited(replicas: i64) -> String {
    format!(
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n  namespace: staging\nspec:\n  replicas: {replicas}\n"
    )
}

/// 2.1: `e` makes the manifest editable; Escape drops the edit, and the next
/// edit starts from the object again, not from the dropped text.
#[gpui_kit::test]
async fn edit_opens_the_manifest_and_cancel_reverts_it(cx: &mut TestAppContext) {
    let mut h = open(cx, deployments(), APPS, "deployments", deployment(2));

    h.press("e");
    let original = h.edit_text().expect("editing");
    assert!(original.contains("replicas: 2"), "{original}");

    h.type_text(&edited(9));
    h.press("escape");
    assert_eq!(h.edit_text(), None, "Escape drops the edit");
    assert!(h.cluster.patches().is_empty(), "and saves nothing");

    h.press("e");
    assert_eq!(
        h.edit_text().as_deref(),
        Some(original.as_str()),
        "reverted"
    );
}

/// 2.2 end to end: `cmd-s` applies the edit, closes the editor, and the panel
/// shows the object as the cluster now has it.
#[gpui_kit::test]
async fn saving_applies_the_edit(cx: &mut TestAppContext) {
    let mut h = open(cx, deployments(), APPS, "deployments", deployment(2));

    h.press("e");
    h.type_text(&edited(5));
    h.press("cmd-s");
    h.wait_for("saved and closed the editor", |panel, _| {
        panel.edit.is_none()
    });

    let patches = h.cluster.patches();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].1["spec"]["replicas"], json!(5));
    h.wait_for("showed the saved object", |panel, _| {
        panel
            .object()
            .is_some_and(|object| object.data["spec"]["replicas"] == json!(5))
    });
}

/// 2.3: text that isn't a manifest for this object can't be saved - a reason,
/// the edit kept, and no request sent.
#[gpui_kit::test]
async fn invalid_yaml_blocks_save_without_a_request(cx: &mut TestAppContext) {
    let mut h = open(cx, deployments(), APPS, "deployments", deployment(2));

    h.press("e");
    h.type_text("spec: [unclosed");
    h.press("cmd-s");

    let failure = h.edit_failure().expect("a reason");
    assert!(failure.starts_with("Not a valid manifest"), "{failure}");
    assert_eq!(
        h.edit_text().as_deref(),
        Some("spec: [unclosed"),
        "the edit stays"
    );
    assert!(h.cluster.patches().is_empty(), "nothing sent");
}

/// 2.4: a refused apply - here a stale resourceVersion - shows the server's
/// reason and keeps the edited text, so the user can retry.
#[gpui_kit::test]
async fn a_conflicting_save_keeps_the_edit(cx: &mut TestAppContext) {
    let mut h = open(cx, deployments(), APPS, "deployments", deployment(2));
    h.cluster.refuse_patches(
        "409 Conflict",
        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
            "reason": "Conflict", "code": 409,
            "message": "Operation cannot be fulfilled on deployments.apps \"web\": the object has been modified; please apply your changes to the latest version and try again" }),
    );

    h.press("e");
    h.type_text(&edited(7));
    h.press("cmd-s");
    h.wait_for("showed the conflict", |panel, _| {
        panel
            .edit
            .as_ref()
            .is_some_and(|edit| edit.failure.is_some() && !edit.saving)
    });

    let failure = h.edit_failure().unwrap();
    assert!(failure.starts_with("Conflict: "), "{failure}");
    assert_eq!(h.edit_text(), Some(edited(7)), "the edit stays for a retry");
}

/// A Secret's values are redacted here, so saving its YAML would overwrite the
/// real ones: Edit says why it won't, and opens nothing.
#[gpui_kit::test]
async fn a_secret_is_not_offered_for_editing(cx: &mut TestAppContext) {
    let secret = json!({
        "apiVersion": "v1", "kind": "Secret",
        "metadata": { "name": "web", "namespace": "staging", "uid": "s1" },
        "data": { "password": "aHVudGVyMg==" },
    });
    let mut h = open(
        cx,
        kind("", "v1", "Secret", true),
        "/api/v1",
        "secrets",
        secret,
    );

    h.press("e");

    assert_eq!(h.edit_text(), None);
    let notice = h.vcx.update(|_, cx| h.panel.read(cx).edit_notice.clone());
    assert!(notice.is_some_and(|notice| notice.contains("Secret")));
}
