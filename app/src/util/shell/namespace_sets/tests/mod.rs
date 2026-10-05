//! `namespace-sets` through a real window and the app's keymap: a window on
//! `demo` (Pods, Services, Nodes) and `other` (Services), Pods focused, `demo`'s
//! cluster reporting `payments`, `team-a`, `team-b` and `team-c`.

use crate::config::namespaces::{NamespaceSetConfig, NamespaceSetsConfig};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::namespaces::{NamespaceList, NamespaceRegistry};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::namespace_sets::store::NamespaceSets;
use crate::ui::nav::OpenedPanel;
use crate::util::shell::namespace_defaults::NamespaceDefaults;
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, NavTarget, WindowMode, init};
use gpui_kit::Focusable as _;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext, WindowHandle};
use kube::core::GroupVersionKind;

mod editor;
mod switch;

fn kind(kind: &str, plural: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", kind),
        plural: plural.into(),
        namespaced,
        verbs: Default::default(),
    }
}

pub(super) fn services() -> DiscoveredKind {
    kind("Service", "services", true)
}
pub(super) fn nodes() -> DiscoveredKind {
    kind("Node", "nodes", false)
}

pub(super) const CLUSTER: [&str; 4] = ["payments", "team-a", "team-b", "team-c"];

pub(super) struct Harness {
    /// Under a `Root`, as `open_window` builds it, so dialogs have a layer.
    pub(super) window: WindowHandle<Root>,
    pub(super) main: Entity<MainWindow>,
    pub(super) vcx: VisualTestContext,
}

pub(super) fn set(name: &str, namespaces: &[&str]) -> NamespaceSetConfig {
    NamespaceSetConfig {
        name: name.into(),
        namespaces: namespaces.iter().map(|n| n.to_string()).collect(),
    }
}

/// The window, with `sets` saved and `keymap` (a `keymap.toml`'s text) in
/// effect.
pub(super) fn harness_with(
    cx: &mut TestAppContext,
    sets: Vec<NamespaceSetConfig>,
    keymap: &str,
) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap_path) = (temp_workspace_path(), temp_workspace_path());
    if !keymap.is_empty() {
        std::fs::write(&keymap_path, keymap).unwrap();
    }
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap_path);
        for context in ["demo", "other"] {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
        }
        let cluster = cx.new(|_| NamespaceList::with_names(&CLUSTER));
        NamespaceRegistry::set_for_test(cx, "demo", cluster);
        NamespaceSets::set_for_test(NamespaceSetsConfig { sets }, cx);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| {
            let mut main =
                MainWindow::test_workspace(vec!["demo".into(), "other".into()], window, cx);
            main.open_target(NavTarget::Kind(services()), window, cx);
            main.open_target(NavTarget::Kind(nodes()), window, cx);
            main.open_target_in(
                NavTarget::Kind(services()),
                None,
                Some("other".into()),
                Vec::new(),
                crate::ui::nav::OpenMode::Foreground,
                window,
                cx,
            );
            main
        });
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its MainWindow");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| main.update(cx, |main, cx| main.test_focus(window, cx)));
    // Pods is the first tab; showing it focuses it.
    press(&mut vcx, "cmd-1");
    let pods = vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            panic!("a workspace window");
        };
        let Some(OpenedPanel::Pods(pods)) = open_panels[0].panel.clone() else {
            panic!("the workspace opens on Pods");
        };
        pods.read(cx).focus_handle(cx)
    });
    assert!(
        vcx.update(|window, cx| pods.contains_focused(window, cx)),
        "Pods is shown and focused"
    );
    Harness { window, main, vcx }
}

/// The two sets most tests start with: `team-workloads` then `payments`.
pub(super) fn harness(cx: &mut TestAppContext) -> Harness {
    harness_with(
        cx,
        vec![
            set("team-workloads", &["team-a", "team-b"]),
            set("payments", &["payments"]),
        ],
        "",
    )
}

pub(super) fn scope_of(h: &mut Harness, context: &str, target: &NavTarget) -> Vec<String> {
    let main = h.main.clone();
    h.vcx
        .update(|_, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
                return None;
            };
            open_panels
                .iter()
                .find(|open| open.key.context_name == context && &open.key.target == target)
                .map(|open| open.key.namespaces.clone())
        })
        .unwrap_or_else(|| panic!("{context} has {target:?} open"))
}

pub(super) fn default_of(h: &mut Harness, context: &str) -> Option<Vec<String>> {
    h.vcx.update(|_, cx| NamespaceDefaults::get(cx, context))
}

/// The saved sets, as name and namespaces.
pub(super) fn saved(h: &mut Harness) -> Vec<(String, Vec<String>)> {
    h.vcx.update(|_, cx| {
        NamespaceSets::get(cx)
            .sets
            .iter()
            .map(|set| (set.name.clone(), set.namespaces.clone()))
            .collect()
    })
}

pub(super) fn dialog_open(h: &mut Harness) -> bool {
    h.vcx
        .update(gpui_kit::component::WindowExt::has_active_dialog)
}

pub(super) fn drawn(h: &mut Harness, selector: String) -> bool {
    let _ = h
        .vcx
        .update_window(h.window.into(), |_, window, cx| window.render_frame(cx));
    h.vcx.debug_bounds(selector.leak()).is_some()
}

pub(super) fn type_text(h: &mut Harness, text: &str) {
    h.vcx.simulate_input(text);
    h.vcx.run_until_parked();
}

pub(super) fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}
