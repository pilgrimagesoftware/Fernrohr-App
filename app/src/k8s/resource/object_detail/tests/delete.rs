//! Delete from the object panel (`k9s-remaining-keybindings`): `ctrl-d` asks,
//! naming the object, Enter deletes it, and the panel - following its object -
//! then shows it deleted rather than its stale fields as if current. A kind
//! discovery lists no `delete` for offers nothing. In a window with the app's
//! keymap, over a fake cluster that watches and records deletes.

use super::fixtures::{deployments, target};
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::{DiscoveredKind, KindVerbs};
use crate::k8s::cluster::discovery_registry::DiscoveryRegistry;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_detail::ObjectDetailPanel;
use crate::k8s::test_cluster::FakeCluster;
use crate::keymap::{self, KeymapConfig};
use crate::ui::detail::lifecycle::Lifecycle;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};
use serde_json::json;

const CONTEXT: &str = "kind-dev";
const APPS: &str = "/apis/apps/v1";

struct Harness {
    cluster: FakeCluster,
    panel: Entity<ObjectDetailPanel>,
    vcx: VisualTestContext,
}

/// The panel open and focused on `staging/web`, a Deployment discovery
/// reports with `delete` or without it, once it has loaded.
fn open(cx: &mut TestAppContext, delete: bool) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_detail::register_commands(&mut registry);
        crate::ui::confirm_dialog::register_commands(&mut registry);
        let bindings = keymap::bindings(
            &registry,
            &KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        APPS,
        "deployments",
        json!({ "apiVersion": "apps/v1", "kind": "Deployment",
            "metadata": { "name": "web", "namespace": "staging", "uid": "d1" },
            "spec": { "replicas": 1, "selector": { "matchLabels": { "app": "web" } }, "template": {} } }),
    );
    let discovered = DiscoveredKind {
        verbs: KindVerbs {
            delete,
            ..KindVerbs::default()
        },
        ..deployments()
    };
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
        DiscoveryRegistry::insert_test(cx, CONTEXT, vec![discovered]);
    });
    // Opened as a restored panel would be, assuming the kind can be deleted:
    // discovery's answer is the one that counts.
    let target = target(deployments(), Some("staging"), "web");
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
    harness.wait_for("loaded the object", |panel, _| {
        matches!(
            panel.state,
            crate::k8s::resource::object_detail::fetch::ObjectDetailState::Loaded(..)
        )
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

    fn deletable_focused(&mut self) -> bool {
        self.vcx.update(|window, _| {
            window
                .context_stack()
                .iter()
                .any(|context| context.contains(super::super::delete::DELETABLE_KEY_CONTEXT))
        })
    }

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
}

/// `ctrl-d` asks; Escape sends nothing, nor Enter - the confirmation opens on
/// Cancel; the shortcut deletes the object, and the panel shows it deleted -
/// and no longer offers Delete.
#[gpui_kit::test]
async fn ctrl_d_deletes_the_object_and_the_panel_shows_it_gone(cx: &mut TestAppContext) {
    let mut harness = open(cx, true);
    assert!(harness.deletable_focused(), "Delete is offered");

    harness.press("ctrl-d");
    assert!(harness.dialog_open(), "Delete asks first");
    harness.press("escape");
    assert!(harness.cluster.deletes().is_empty(), "Escape sends nothing");

    harness.press("ctrl-d");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("enter");
    assert!(!harness.dialog_open(), "Enter closes it");
    assert!(
        harness.cluster.deletes().is_empty(),
        "on Cancel, sending nothing"
    );

    harness.press("ctrl-d");
    crate::ui::confirm_dialog::deliver_first_frame(&mut harness.vcx);
    harness.press("secondary-backspace");
    harness.wait_for("showed the object deleted", |panel, _| {
        matches!(panel.lifecycle, Some(Lifecycle::Deleted { .. }))
    });
    let deletes = harness.cluster.deletes();
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].0, "web");
    assert!(
        !harness.deletable_focused(),
        "a deleted object offers no Delete"
    );
}

/// Discovery lists no `delete` for the kind: no Delete context, and `ctrl-d`
/// neither asks nor deletes.
#[gpui_kit::test]
async fn a_kind_without_delete_offers_no_delete(cx: &mut TestAppContext) {
    let mut harness = open(cx, false);
    assert!(!harness.deletable_focused(), "no Delete context");
    harness.press("ctrl-d");
    assert!(!harness.dialog_open(), "no question");
    assert!(harness.cluster.deletes().is_empty(), "nothing deleted");
}

/// With a YAML edit open, Delete isn't offered - even with focus outside the
/// editor, on the panel itself, where `!Input` doesn't hold it back - so
/// `ctrl-d` can't discard the unsaved edit.
#[gpui_kit::test]
async fn ctrl_d_with_an_edit_open_does_nothing(cx: &mut TestAppContext) {
    let mut harness = open(cx, true);
    harness.press("e");
    let panel = harness.panel.clone();
    assert!(
        harness
            .vcx
            .update(|_, cx| panel.read(cx).edit_text(cx).is_some()),
        "`e` opened an edit"
    );
    harness.vcx.update(|window, cx| {
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    harness.vcx.run_until_parked();
    assert!(!harness.deletable_focused(), "no Delete context");

    harness.press("ctrl-d");
    assert!(!harness.dialog_open(), "no question");
    assert!(harness.cluster.deletes().is_empty(), "nothing deleted");
    assert!(
        harness
            .vcx
            .update(|_, cx| panel.read(cx).edit_text(cx).is_some()),
        "the edit is still open"
    );
}
