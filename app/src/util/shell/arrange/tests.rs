//! The arrange commands through a real window and the app's keymap: Split,
//! Move and Merge in each case the spec names, and Close Group with nothing to
//! lose, and with an unsaved edit - asking once, Cancel closing nothing.

use crate::command::CommandRegistry;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::{NavTarget, ObjectTarget, OpenedPanel};
use crate::ui::panel::arrange::{DOCK_COMMANDS_CONTEXT, DOCK_KEY_CONTEXT, group_rects};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::dock::{DockPlacement, NodeId, PaneRef, PanelId};
use gpui_kit::component::{Root, WindowExt as _};
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

fn deployment_target() -> ObjectTarget {
    ObjectTarget {
        kind: DiscoveredKind {
            gvk: GroupVersionKind::gvk("apps", "v1", "Deployment"),
            plural: "deployments".into(),
            namespaced: true,
            verbs: Default::default(),
        },
        namespace: Some("staging".into()),
        name: "web".into(),
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

    fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    fn press_dialog_button(&mut self, n: usize) {
        for _ in 0..n {
            self.press("tab");
        }
        let space = gpui_kit::Keystroke::parse("space").expect("valid");
        self.vcx.simulate_event(gpui_kit::KeyDownEvent {
            keystroke: space.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        self.vcx
            .simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
        self.vcx.run_until_parked();
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

/// 3.1: Move Right takes the focused panel into the group to the right; moving
/// a group's last panel removes its pane; with nothing that way, nothing moves.
#[gpui_kit::test]
async fn move_takes_the_panel_beside_and_an_emptied_pane_goes(cx: &mut TestAppContext) {
    let mut h = harness(cx, Some(svc()));
    h.press("cmd-k right");
    h.press("cmd-alt-left");
    assert_eq!(
        h.groups(),
        [vec![pods(), svc(), svc()]],
        "the copy moved back, its pane gone"
    );

    h.press("cmd-k right");
    assert_eq!(h.groups(), [vec![pods(), svc(), svc()], vec![svc()]]);
    h.press("cmd-alt-right");
    assert_eq!(
        h.groups(),
        [vec![pods(), svc(), svc()], vec![svc()]],
        "nothing to the right of the right pane: unchanged"
    );
    h.press("cmd-alt-left");
    assert_eq!(h.groups(), [vec![pods(), svc(), svc(), svc()]]);
    assert_eq!(
        h.focused().map(|(_, target)| target),
        Some(svc()),
        "the moved panel has focus"
    );
}

/// 5.1: Merge Right moves the focused group's panels, in order, into the group
/// to the right and removes the pane; with nothing that way, nothing changes.
#[gpui_kit::test]
async fn merge_joins_the_group_beside_in_order(cx: &mut TestAppContext) {
    let mut h = harness(cx, Some(svc()));
    h.press("cmd-k right");
    h.press("cmd-alt-shift-right");
    assert_eq!(
        h.groups(),
        [vec![pods(), svc()], vec![svc()]],
        "nothing right of the right pane: unchanged"
    );

    h.press("cmd-alt-shift-left");
    assert_eq!(
        h.groups(),
        [vec![pods(), svc(), svc()]],
        "appended, in order"
    );
}

/// 4.1: a group with nothing to lose closes at once; one with an unsaved edit
/// asks once - Cancel closes nothing, Close Group closes every panel in it.
#[gpui_kit::test]
async fn close_group_asks_only_when_something_would_be_lost(cx: &mut TestAppContext) {
    let mut h = harness(cx, Some(svc()));
    h.press("cmd-k right");
    h.press("cmd-k w");
    assert!(!h.dialog_open(), "nothing to lose: no question");
    assert_eq!(h.groups(), [vec![pods(), svc()]], "the right pane closed");

    // A Deployment open, loaded, and being edited, in a pane of its own.
    let main = h.main.clone();
    h.vcx.update(|window, cx| {
        main.update(cx, |main, cx| {
            main.open_target(NavTarget::Object(deployment_target()), window, cx)
        })
    });
    h.vcx.run_until_parked();
    h.press("cmd-k down");
    let object = h.vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return None;
        };
        open_panels.iter().rev().find_map(|open| match &open.panel {
            Some(OpenedPanel::ObjectDetail(panel)) => Some(panel.clone()),
            _ => None,
        })
    });
    let object = object.expect("the split's Deployment panel");
    h.vcx.update(|_, cx| {
        object.update(cx, |panel, cx| {
            let loaded = serde_json::from_value(serde_json::json!({
                "apiVersion": "apps/v1", "kind": "Deployment",
                "metadata": { "name": "web", "namespace": "staging" },
            }))
            .unwrap();
            panel.test_set_loaded(loaded, cx);
        })
    });
    h.press("e");
    assert!(
        h.vcx
            .update(|_, cx| object.read(cx).close_warning().is_some()),
        "editing"
    );
    // Out of the editor - a text field, where the arrange keys don't fire -
    // and onto the panel, the edit still open.
    h.vcx.update(|window, cx| {
        use gpui_kit::Focusable as _;
        let focus = object.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    let groups_before = h.groups();

    h.press("cmd-k w");
    assert!(h.dialog_open(), "an unsaved edit: it asks");
    h.press_dialog_button(1);
    assert!(!h.dialog_open());
    assert_eq!(h.groups(), groups_before, "Cancel closes nothing");

    h.press("cmd-k w");
    h.press_dialog_button(2);
    assert_eq!(
        h.groups().len(),
        groups_before.len() - 1,
        "Close Group closed the pane"
    );
}

/// 2.2, 3.2, 4.2, 5.2: every arrange command is offered in the dock's context
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
    assert_eq!(ids.len(), 13, "{ids:?}");
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
/// With the Pods panel's namespace filter focused, split, move and close-group
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
    h.press("cmd-alt-left");
    h.press("cmd-k w");

    assert_eq!(h.groups(), before, "the layout is unchanged");
    assert!(!h.dialog_open());
}
