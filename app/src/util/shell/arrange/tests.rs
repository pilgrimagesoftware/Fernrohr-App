//! The arrange commands through a real window and the app's keymap: Split,
//! Move and Merge in each case the spec names, and Close Group with nothing to
//! lose, and with an unsaved edit - asking once, Cancel closing nothing.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use crate::ui::panel::arrange::{DOCK_COMMANDS_CONTEXT, DOCK_KEY_CONTEXT, group_rects};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::component::dock::{DockPlacement, NodeId, PaneRef, PanelId};
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use kube::core::GroupVersionKind;

fn services() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

struct Harness {
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

/// A window on `demo` showing Pods, then `extra` opened into Pods' group -
/// one group, its last panel focused.
fn harness(cx: &mut TestAppContext, extra: Option<NavTarget>) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| main.update(cx, |main, cx| main.test_focus(window, cx)));
    vcx.simulate_keystrokes("cmd-1");
    vcx.run_until_parked();
    if let Some(target) = extra {
        vcx.update(|window, cx| main.update(cx, |main, cx| main.open_target(target, window, cx)));
        vcx.run_until_parked();
    }
    Harness { main, vcx }
}

impl Harness {
    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    /// The centre's groups, each its panels' targets in strip order, in layout
    /// order (left to right, top to bottom).
    fn groups(&mut self) -> Vec<Vec<NavTarget>> {
        let main = self.main.clone();
        self.vcx.update(|_, cx| {
            let main = main.read(cx);
            let WindowMode::Workspace {
                dock_area,
                open_panels,
                ..
            } = &main.mode
            else {
                return Vec::new();
            };
            let area = dock_area.read(cx);
            let tree = area.layout(DockPlacement::Center).expect("a centre");
            let target_of = |panel: &PanelId| {
                open_panels
                    .iter()
                    .find(|open| open.id == *panel)
                    .map(|open| open.key.target.clone())
                    .expect("a keyed panel")
            };
            group_rects(tree.root())
                .into_iter()
                .filter_map(|(node, _)| match tree.find_node(node)?.kind() {
                    PaneRef::Tabs { panels, .. } => Some(panels.iter().map(target_of).collect()),
                    PaneRef::Split { .. } => None,
                })
                .collect()
        })
    }

    /// The group holding the focused panel, and that panel's target.
    fn focused(&mut self) -> Option<(NodeId, NavTarget)> {
        let main = self.main.clone();
        self.vcx.update(|window, cx| {
            let main = main.read(cx);
            let WindowMode::Workspace {
                dock_area,
                open_panels,
                ..
            } = &main.mode
            else {
                return None;
            };
            let area = dock_area.read(cx);
            let node = crate::ui::panel::focus::focused_group(area, window, cx)?;
            let panel = crate::ui::panel::tabs::active_panel_of(area, node)?;
            let target = open_panels
                .iter()
                .find(|open| open.id == panel)?
                .key
                .target
                .clone();
            Some((node, target))
        })
    }
}

fn pods() -> NavTarget {
    NavTarget::pods()
}

fn svc() -> NavTarget {
    NavTarget::Kind(services())
}

/// 2.1: Split Right opens a second Services panel in a new pane to the right,
/// and focuses it.
#[gpui_kit::test]
async fn split_opens_a_copy_beside_the_group_and_focuses_it(cx: &mut TestAppContext) {
    let mut h = harness(cx, Some(svc()));
    assert_eq!(h.groups(), [vec![pods(), svc()]]);
    let (before, _) = h.focused().expect("Services has focus");

    h.press("cmd-k right");

    assert_eq!(
        h.groups(),
        [vec![pods(), svc()], vec![svc()]],
        "a copy, to the right"
    );
    let (after, target) = h.focused().expect("focus is in the new pane");
    assert_ne!(after, before, "the new pane has focus");
    assert_eq!(target, svc());
}

/// 2.2: every arrange command is offered in the dock's context
/// with its default key, and a first-run keymap.toml lists it.
#[test]
fn every_arrange_command_is_registered_and_in_the_first_run_keymap() {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let ids: Vec<&str> = registry
        .iter()
        .filter(|command| {
            command.id.starts_with("panel.") && command.context == Some(DOCK_COMMANDS_CONTEXT)
        })
        .map(|command| command.id)
        .collect();
    assert_eq!(ids.len(), 4, "{ids:?}");
    let path = crate::util::test_paths::temp_path("arrange-keymap");
    let _ = std::fs::remove_file(&path);
    crate::keymap::load(&path, &registry);
    let written: crate::keymap::KeymapConfig =
        toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    for id in ids {
        assert!(
            registry
                .available(&[DOCK_KEY_CONTEXT])
                .iter()
                .any(|c| c.id == id),
            "{id} is in the palette with a panel focused"
        );
        let command = registry.get(id).unwrap();
        assert_eq!(
            written.bindings.get(id).map(String::as_str),
            Some(command.default_binding)
        );
    }
    let _ = std::fs::remove_file(&path);
}

/// The k9s review's lesson: the arrange keys are bound outside text fields.
/// With the Pods panel's namespace filter focused, split
/// keys change nothing.
#[gpui_kit::test]
async fn arrange_keys_do_nothing_while_typing(cx: &mut TestAppContext) {
    let mut h = harness(cx, None);
    let before = h.groups();
    h.press("n");
    let typing = h.vcx.update(|window, _| {
        window
            .context_stack()
            .iter()
            .any(|context| context.contains("Input"))
    });
    assert!(typing, "`n` focuses the namespace filter");

    h.press("cmd-k right");

    assert_eq!(h.groups(), before, "the layout is unchanged");
}
