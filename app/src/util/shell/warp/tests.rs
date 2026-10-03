//! `warp-all-to-namespace` through a real window and the app's keymap: Warp All
//! (`shift-w`) on a Pods panel moves every namespaced list in its context and sets
//! the context's default; `w` still moves only the focused panel.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::util::shell::namespace_defaults::NamespaceDefaults;
use crate::util::shell::test_support::{handles, press, temp_workspace_path};
use crate::util::shell::{MainWindow, NavTarget, WindowMode, init, register_commands};
use gpui_kit::{TestAppContext, VisualTestContext, WindowHandle};
use kube::core::GroupVersionKind;

fn kind(group: &str, kind: &str, plural: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: plural.into(),
        namespaced,
        verbs: Default::default(),
    }
}

fn services() -> DiscoveredKind {
    kind("", "Service", "services", true)
}
fn nodes() -> DiscoveredKind {
    kind("", "Node", "nodes", false)
}
fn configmaps() -> DiscoveredKind {
    kind("", "ConfigMap", "configmaps", true)
}

struct Harness {
    window: WindowHandle<MainWindow>,
    vcx: VisualTestContext,
}

/// A window on `demo` and `other`: Pods, Services and Nodes in `demo`, Services in
/// `other`, and a selected pod in `team-a`. Pods shown and focused.
fn harness(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        for context in ["demo", "other"] {
            ClusterRegistry::insert_test_session(cx, context, ConnectionState::Connecting);
        }
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "team-a".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "demo".into(),
        })));
    });
    let window = cx.add_window(|window, cx| {
        let mut main = MainWindow::test_workspace(vec!["demo".into(), "other".into()], window, cx);
        main.open_target(NavTarget::Kind(services()), window, cx);
        main.open_target(NavTarget::Kind(nodes()), window, cx);
        main.open_target_in(
            NavTarget::Kind(services()),
            None,
            Some("other".into()),
            Vec::new(),
            window,
            cx,
        );
        main
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    // Show Pods (cmd-1): it's a hidden tab behind the lists opened after it, and a
    // hidden panel's keys reach nothing. Showing it focuses it.
    window
        .update(&mut vcx, |main, window, cx| main.test_focus(window, cx))
        .unwrap();
    press(&mut vcx, "cmd-1");
    let pods = handles(window, &mut vcx).pods;
    assert!(
        window
            .update(&mut vcx, |_, window, cx| pods.contains_focused(window, cx))
            .unwrap(),
        "Pods is shown and focused"
    );
    Harness { window, vcx }
}

/// Each open panel's context, target and namespace scope.
fn scopes(h: &mut Harness) -> Vec<(String, NavTarget, Vec<String>)> {
    h.window
        .update(&mut h.vcx, |main, _, _| {
            let WindowMode::Workspace { open_panels, .. } = &main.mode else {
                return Vec::new();
            };
            open_panels
                .iter()
                .map(|open| {
                    (
                        open.key.context_name.clone(),
                        open.key.target.clone(),
                        open.key.namespaces.clone(),
                    )
                })
                .collect()
        })
        .unwrap()
}

fn scope_of(h: &mut Harness, context: &str, target: &NavTarget) -> Vec<String> {
    scopes(h)
        .into_iter()
        .find(|(c, t, _)| c == context && t == target)
        .map(|(_, _, namespaces)| namespaces)
        .unwrap_or_else(|| panic!("{context} has {target:?} open"))
}

fn default_of(h: &mut Harness, context: &str) -> Option<Vec<String>> {
    h.vcx.update(|_, cx| NamespaceDefaults::get(cx, context))
}

/// 2.2 and 1.2: Warp All moves every namespaced list in the context, leaves the
/// cluster-scoped list and the other context alone, sets the default - and a list
/// opened afterwards starts in it.
#[gpui_kit::test]
async fn warp_all_moves_the_context_and_sets_its_default(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let team_a = vec!["team-a".to_string()];

    press(&mut h.vcx, "shift-w");

    assert_eq!(scope_of(&mut h, "demo", &NavTarget::pods()), team_a, "Pods");
    assert_eq!(
        scope_of(&mut h, "demo", &NavTarget::Kind(services())),
        team_a,
        "Services"
    );
    assert!(
        scope_of(&mut h, "demo", &NavTarget::Kind(nodes())).is_empty(),
        "Nodes kept"
    );
    assert!(
        scope_of(&mut h, "other", &NavTarget::Kind(services())).is_empty(),
        "another context's panels are untouched"
    );
    assert_eq!(default_of(&mut h, "demo"), Some(team_a.clone()));
    assert_eq!(default_of(&mut h, "other"), None);

    h.window
        .update(&mut h.vcx, |main, window, cx| {
            main.open_target(NavTarget::Kind(configmaps()), window, cx)
        })
        .unwrap();
    assert_eq!(
        scope_of(&mut h, "demo", &NavTarget::Kind(configmaps())),
        team_a,
        "a list opened afterwards starts in the default"
    );
}

/// 1.2's other half: with no default set, a new list opens on all namespaces.
#[gpui_kit::test]
async fn with_no_default_a_new_list_opens_on_all_namespaces(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.window
        .update(&mut h.vcx, |main, window, cx| {
            main.open_target(NavTarget::Kind(configmaps()), window, cx)
        })
        .unwrap();
    assert!(scope_of(&mut h, "demo", &NavTarget::Kind(configmaps())).is_empty());
}

/// 3.1: the focused-panel warp (`w`) is unchanged - only that panel moves, and no
/// default is set.
#[gpui_kit::test]
async fn the_focused_warp_stays_local(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(&mut h.vcx, "w");
    assert_eq!(scope_of(&mut h, "demo", &NavTarget::pods()), ["team-a"]);
    assert!(scope_of(&mut h, "demo", &NavTarget::Kind(services())).is_empty());
    assert_eq!(default_of(&mut h, "demo"), None);
}

/// 2.1: Warp All is a palette command scoped to the Pods panel, out of the menu
/// bar, and its key collides with nothing.
#[test]
fn warp_all_is_a_pods_scoped_palette_command() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let command = registry.get("pods.warp_all_namespace").expect("registered");
    assert_eq!(
        command.context,
        Some(crate::k8s::resource::pods::PANEL_KEY_CONTEXT)
    );
    assert_eq!(
        command.menu, None,
        "panel-scoped: the palette, not the menu bar"
    );
    assert!(
        registry
            .available(&[crate::k8s::resource::pods::PANEL_KEY_CONTEXT])
            .iter()
            .any(|available| available.id == "pods.warp_all_namespace"),
        "offered in the palette while a Pods panel has focus"
    );
}

/// 1.1: the default is saved with the workspace, and loaded back on launch.
#[gpui_kit::test]
async fn the_default_is_saved_and_restored_with_the_workspace(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(&mut h.vcx, "shift-w");
    let path = temp_workspace_path();
    h.vcx
        .update(|_, cx| crate::util::shell::persist::save(cx, &path));

    let saved: crate::config::workspace::WorkspaceConfig = crate::config::load(&path);
    assert_eq!(
        saved.namespace_defaults.get("demo"),
        Some(&vec!["team-a".to_string()])
    );

    h.vcx.update(|_, cx| {
        NamespaceDefaults::load(cx, Default::default());
        assert_eq!(NamespaceDefaults::get(cx, "demo"), None);
        let loaded: crate::config::workspace::WorkspaceConfig = crate::config::load(&path);
        NamespaceDefaults::load(cx, loaded.namespace_defaults);
    });
    assert_eq!(default_of(&mut h, "demo"), Some(vec!["team-a".to_string()]));
}
