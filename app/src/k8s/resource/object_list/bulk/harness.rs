//! A kind's list panel in a window with the app's list keymap - the bulk
//! actions bound to keys, as a user's `keymap.toml` could - over a fake cluster
//! that watches, and records and applies patches.

use super::super::panel::ObjectListPanel;
use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
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
use serde_json::{Value, json};

const CONTEXT: &str = "kind-dev";

/// The keys the tests press for the bulk actions, which have none by default:
/// function keys, which no default binding uses on any platform.
const BULK_KEYS: [(&str, &str); 6] = [
    ("bulk.restart_rollout", "f15"),
    ("bulk.scale", "f16"),
    ("bulk.cordon", "f17"),
    ("bulk.drain", "f18"),
    ("bulk.label", "f19"),
    ("bulk.copy_yaml", "f20"),
];

pub(super) fn kind(group: &str, kind: &str, plural: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: plural.into(),
        namespaced,
        verbs: Default::default(),
    }
}

/// `name` of `kind`, in namespace `shop` unless cluster-scoped.
pub(super) fn object(kind: &DiscoveredKind, name: &str) -> Value {
    let api_version = kind.gvk.api_version();
    let mut object = json!({ "apiVersion": api_version, "kind": kind.gvk.kind,
        "metadata": { "name": name, "uid": format!("uid-{name}") } });
    if kind.namespaced {
        object["metadata"]["namespace"] = json!("shop");
    }
    object
}

fn prefix(kind: &DiscoveredKind) -> String {
    if kind.gvk.group.is_empty() {
        "/api/v1".into()
    } else {
        format!("/apis/{}/v1", kind.gvk.group)
    }
}

pub(super) struct Harness {
    pub(super) cluster: FakeCluster,
    pub(super) panel: Entity<ObjectListPanel>,
    pub(super) vcx: VisualTestContext,
}

/// A list of `kind` over objects `names`, the first selected.
pub(super) fn open(cx: &mut TestAppContext, kind: DiscoveredKind, names: &[&str]) -> Harness {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        crate::k8s::resource::object_list::register_commands(&mut registry);
        crate::ui::confirm_dialog::register_commands(&mut registry);
        crate::ui::bulk::register_commands(&mut registry);
        let keymap = KeymapConfig {
            bindings: BULK_KEYS
                .iter()
                .map(|(id, key)| (id.to_string(), key.to_string()))
                .collect(),
        };
        let bindings = keymap::bindings(&registry, &keymap, cx.keyboard_mapper().as_ref());
        cx.bind_keys(bindings);
        cx.bind_keys(crate::ui::list_keys::bindings(&[
            crate::k8s::resource::object_list::LIST_KEY_CONTEXT,
        ]));
    });
    let (cluster, client) = FakeCluster::start(cx);
    for name in names {
        cluster.apply(&prefix(&kind), &kind.plural, object(&kind, name));
    }
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, CONTEXT, ConnectionState::Connected(client));
    });
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
    let count = names.len();
    harness.wait_for("listed every object", |harness| harness.rows() == count);
    let panel = harness.panel.clone();
    harness.vcx.update(|window, cx| {
        window.activate_window();
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    harness.press("down");
    harness
}

impl Harness {
    pub(super) fn press(&mut self, keys: &str) {
        for key in keys.split(' ') {
            self.vcx
                .simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
        }
        self.vcx.run_until_parked();
    }

    /// Checks the first `count` rows with `x`, from the cursor down.
    pub(super) fn check_first(&mut self, count: usize) {
        for n in 0..count {
            if n > 0 {
                self.press("down");
            }
            self.press("x");
        }
    }

    fn rows(&mut self) -> usize {
        let panel = self.panel.clone();
        self.vcx
            .update(|_, cx| panel.read(cx).visible_rows(cx).len())
    }

    pub(super) fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    pub(super) fn in_context(&mut self, context: &str) -> bool {
        self.vcx.update(|window, _| {
            window
                .context_stack()
                .iter()
                .any(|stack| stack.contains(context))
        })
    }

    pub(super) fn drawn(&mut self, selector: &'static str) -> bool {
        self.vcx.update(|window, cx| window.render_frame(cx));
        self.vcx.debug_bounds(selector).is_some()
    }

    /// The outcome banner's summary, once the batch has finished.
    pub(super) fn outcome(&mut self) -> String {
        let panel = self.panel.clone();
        self.wait_for("finished the batch", |harness| {
            harness.vcx.update(|_, cx| {
                panel
                    .read(cx)
                    .bulk_outcome
                    .as_ref()
                    .is_some_and(|outcome| outcome.running.is_none())
            })
        });
        self.vcx.update(|_, cx| {
            panel
                .read(cx)
                .bulk_outcome
                .as_ref()
                .map(|outcome| outcome.summary())
                .unwrap_or_default()
        })
    }

    /// The patches sent so far, by object name, sorted.
    pub(super) fn patches(&mut self, count: usize) -> Vec<(String, Value)> {
        self.wait_for("sent the patches", |harness| {
            harness.cluster.patches().len() >= count
        });
        let mut patches = self.cluster.patches();
        patches.sort_by(|a, b| a.0.cmp(&b.0));
        patches
    }

    pub(super) fn wait_for(&mut self, what: &str, done: impl Fn(&mut Self) -> bool) {
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
