//! Delete from a kind's list, in a window with the app's list keymap over a
//! fake cluster that watches, applies and records deletes. The user's case is
//! a Secret deleted from the Secrets list: `ctrl-d` asks, naming it, Escape
//! sends nothing, Enter deletes it, and the watch drops its row. A kind
//! discovery lists no `delete` for offers nothing, and `ctrl-d` typed into
//! the filter types there.

use super::super::panel::ObjectListPanel;
use super::{DELETABLE_KEY_CONTEXT, REFUSAL_ID};
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::{DiscoveredKind, KindVerbs};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, TestAppContext, VisualTestContext,
};
use kube::core::GroupVersionKind;
use serde_json::json;

const CONTEXT: &str = "kind-dev";

fn secrets(delete: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Secret"),
        plural: "secrets".into(),
        namespaced: true,
        verbs: KindVerbs {
            delete,
            ..KindVerbs::default()
        },
    }
}

fn secret(name: &str, uid: &str) -> serde_json::Value {
    json!({ "apiVersion": "v1", "kind": "Secret", "type": "Opaque",
        "metadata": { "name": name, "namespace": "payments", "uid": uid },
        "data": { "password": "aHVudGVyMg==" } })
}

struct Harness {
    cluster: FakeCluster,
    panel: Entity<ObjectListPanel>,
    vcx: VisualTestContext,
}

/// A Secrets list on `kind-dev`, watching a fake cluster holding two Secrets,
/// with the first selected.
fn open(cx: &mut TestAppContext, delete: bool) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_list::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
        cx.bind_keys(crate::ui::list_keys::bindings(&[
            crate::k8s::resource::object_list::LIST_KEY_CONTEXT,
        ]));
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply("/api/v1", "secrets", secret("db-password", "s1"));
    cluster.apply("/api/v1", "secrets", secret("tls-cert", "s2"));
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
    let kind = secrets(delete);
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Kind(kind.clone()), CONTEXT.into());
        let panel = cx.new(|cx| ObjectListPanel::new(kind, scope, cx));
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
    harness.wait_for("listed both Secrets", |harness| harness.rows().len() == 2);
    let panel = harness.panel.clone();
    harness.vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    harness.press("down");
    harness
}

impl Harness {
    /// Whether the Delete command's context is on the focus path.
    fn deletable_focused(&mut self) -> bool {
        self.vcx.update(|window, _| {
            window
                .context_stack()
                .iter()
                .any(|context| context.contains(DELETABLE_KEY_CONTEXT))
        })
    }

    fn press(&mut self, keys: &str) {
        for key in keys.split(' ') {
            self.vcx
                .simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
        }
        self.vcx.run_until_parked();
    }

    fn rows(&mut self) -> Vec<String> {
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| {
            panel
                .read(cx)
                .visible_rows(cx)
                .into_iter()
                .map(|row| row.object.name)
                .collect()
        })
    }

    fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    fn wait_for(&mut self, what: &str, done: impl Fn(&mut Self) -> bool) {
        for _ in 0..400 {
            self.vcx.run_until_parked();
            if done(self) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never {what}");
    }
}

/// The user's case: `ctrl-d` on a Secret asks first; Escape sends nothing;
/// `ctrl-d` then Enter deletes it with a plain delete, and its row goes.
#[gpui_kit::test]
async fn ctrl_d_deletes_a_secret_from_the_secrets_list(cx: &mut TestAppContext) {
    let mut harness = open(cx, true);
    assert!(
        harness.deletable_focused(),
        "the selected row can be deleted"
    );

    harness.press("ctrl-d");
    assert!(harness.dialog_open(), "Delete asks first");
    harness.press("escape");
    assert!(!harness.dialog_open(), "Escape closes it");
    assert!(harness.cluster.deletes().is_empty(), "and sends nothing");

    harness.press("ctrl-d");
    harness.press("enter");
    assert!(!harness.dialog_open(), "Enter confirms and closes it");
    harness.wait_for("dropped the deleted Secret's row", |harness| {
        harness.rows() == ["tls-cert"]
    });
    let deletes = harness.cluster.deletes();
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].0, "db-password");
    assert_eq!(
        deletes[0].1.get("gracePeriodSeconds"),
        None,
        "a plain delete"
    );
}

/// A refused delete shows the server's reason above the table, and the row
/// stays.
#[gpui_kit::test]
async fn a_refused_delete_shows_why_and_keeps_the_row(cx: &mut TestAppContext) {
    let mut harness = open(cx, true);
    harness.cluster.refuse_deletes(
        "403 Forbidden",
        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
            "reason": "Forbidden", "code": 403,
            "message": "secrets \"db-password\" is forbidden" }),
    );

    harness.press("ctrl-d");
    harness.press("enter");
    let panel = harness.panel.clone();
    harness.wait_for("showed the refusal", |harness| {
        harness.vcx.update(|_, cx| panel.read(cx).refusal.is_some())
    });
    let action = harness
        .vcx
        .update(|_, cx| panel.read(cx).refusal.clone().map(|refusal| refusal.action));
    assert_eq!(action.as_deref(), Some("Delete Secret db-password"));
    harness.vcx.update(|window, cx| window.render_frame(cx));
    assert!(
        harness.vcx.debug_bounds(REFUSAL_ID).is_some(),
        "the banner is drawn"
    );
    assert_eq!(harness.rows().len(), 2, "the row stays");
}

/// A kind discovery lists no `delete` for: the list has no deletable context,
/// so `ctrl-d` neither asks nor deletes.
#[gpui_kit::test]
async fn a_kind_without_delete_offers_no_delete(cx: &mut TestAppContext) {
    let mut harness = open(cx, false);
    assert!(!harness.deletable_focused(), "no Delete context");

    harness.press("ctrl-d");
    assert!(!harness.dialog_open(), "no question");
    assert!(harness.cluster.deletes().is_empty(), "nothing deleted");
}

/// `ctrl-d` typed into the filter stays there: no question, no delete.
#[gpui_kit::test]
async fn ctrl_d_in_the_filter_does_not_delete(cx: &mut TestAppContext) {
    let mut harness = open(cx, true);
    harness.press("/");
    let in_filter = harness.vcx.update(|window, _| {
        window
            .context_stack()
            .iter()
            .any(|context| context.contains("Input"))
    });
    assert!(in_filter, "`/` focuses the filter");

    harness.press("ctrl-d");
    assert!(!harness.dialog_open(), "no question");
    assert!(harness.cluster.deletes().is_empty(), "nothing deleted");
}
