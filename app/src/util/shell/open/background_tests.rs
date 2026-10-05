//! `open-in-background` 1.1: a background open adds the panel as an inactive tab
//! and leaves focus where it was - even when the new tab joins the focused list's
//! own group - an already-open panel is left alone, and a foreground open still
//! shows and focuses its panel.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::{NavTarget, OpenMode};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::component::dock::DockPlacement;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use kube::core::GroupVersionKind;

fn services() -> NavTarget {
    NavTarget::Kind(DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
        verbs: Default::default(),
    })
}

struct Harness {
    main: Entity<MainWindow>,
    vcx: VisualTestContext,
}

/// A window on `demo` showing Pods, focused.
fn harness(cx: &mut TestAppContext) -> Harness {
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
    Harness { main, vcx }
}

impl Harness {
    fn open(&mut self, target: NavTarget, mode: OpenMode) {
        let main = self.main.clone();
        self.vcx.update(|window, cx| {
            main.update(cx, |main, cx| {
                main.open_target_in(
                    target,
                    None,
                    Some("demo".into()),
                    Vec::new(),
                    mode,
                    window,
                    cx,
                )
            })
        });
        self.vcx.run_until_parked();
    }

    /// Each centre group's tabs (targets in strip order) and the one it shows.
    fn groups(&mut self) -> Vec<(Vec<NavTarget>, NavTarget)> {
        let main = self.main.clone();
        self.vcx.update(|_, cx| {
            let WindowMode::Workspace {
                dock_area,
                open_panels,
                ..
            } = &main.read(cx).mode
            else {
                return Vec::new();
            };
            let area = dock_area.read(cx);
            let tree = area.layout(DockPlacement::Center).expect("a centre");
            let target_of = |panel| {
                open_panels
                    .iter()
                    .find(|open| open.id == panel)
                    .map(|open| open.key.target.clone())
                    .expect("a keyed panel")
            };
            crate::ui::panel::arrange::group_rects(tree.root())
                .into_iter()
                .filter_map(|(node, _)| {
                    let group = crate::ui::panel::tabs::tabs_of(area, node)?;
                    let shown = target_of(*group.panels.get(group.active_ix)?);
                    Some((group.panels.into_iter().map(target_of).collect(), shown))
                })
                .collect()
        })
    }

    /// The target of the panel focus is in.
    fn focused(&mut self) -> Option<NavTarget> {
        let main = self.main.clone();
        self.vcx.update(|window, cx| {
            let WindowMode::Workspace {
                dock_area,
                open_panels,
                ..
            } = &main.read(cx).mode
            else {
                return None;
            };
            let area = dock_area.read(cx);
            let node = crate::ui::panel::focus::focused_group(area, window, cx)?;
            let panel = crate::ui::panel::tabs::active_panel_of(area, node)?;
            Some(
                open_panels
                    .iter()
                    .find(|open| open.id == panel)?
                    .key
                    .target
                    .clone(),
            )
        })
    }
}

#[gpui_kit::test]
async fn a_background_open_adds_an_inactive_tab_and_keeps_focus(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    assert_eq!(h.focused(), Some(NavTarget::pods()));

    h.open(services(), OpenMode::Background);

    assert_eq!(
        h.groups(),
        [(vec![NavTarget::pods(), services()], NavTarget::pods())],
        "the new tab joined the list's group, and the list is still shown"
    );
    assert_eq!(
        h.focused(),
        Some(NavTarget::pods()),
        "focus stayed on the list"
    );
}

#[gpui_kit::test]
async fn a_background_open_of_an_open_panel_changes_nothing(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.open(services(), OpenMode::Foreground);
    h.open(NavTarget::pods(), OpenMode::Foreground);
    let before = h.groups();
    assert_eq!(h.focused(), Some(NavTarget::pods()));

    h.open(services(), OpenMode::Background);

    assert_eq!(h.groups(), before, "Services' tab was not shown");
    assert_eq!(h.focused(), Some(NavTarget::pods()));
}

#[gpui_kit::test]
async fn a_foreground_open_still_shows_and_focuses(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.open(services(), OpenMode::Foreground);
    assert_eq!(
        h.groups(),
        [(vec![NavTarget::pods(), services()], services())]
    );
    assert_eq!(h.focused(), Some(services()));
}

/// 2.2: the window turns a Pods row's background open into the pod's detail as an
/// inactive tab, focus left on the list.
#[gpui_kit::test]
async fn a_pod_opened_in_the_background_is_an_inactive_tab(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    h.vcx.dispatch_action(crate::ui::nav::OpenPodInBackground {
        context_name: "demo".into(),
        namespace: "shop".into(),
        name: "web-1".into(),
    });
    h.vcx.run_until_parked();

    assert_eq!(
        h.groups(),
        [(
            vec![NavTarget::pods(), NavTarget::pod("shop", "web-1")],
            NavTarget::pods()
        )]
    );
    assert_eq!(h.focused(), Some(NavTarget::pods()));
}

/// 2.1: neither list's Open in Background key clashes with a registered default -
/// the same key, or one starting another's chord.
#[test]
fn the_open_in_background_keys_clash_with_nothing() {
    let mut registry = crate::command::CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    for id in ["pods.open_in_background", "object_list.open_in_background"] {
        let command = registry.get(id).expect("registered");
        assert!(
            command
                .context
                .is_some_and(|context| context.contains("!Input"))
        );
        let clashes = crate::keymap::conflicts(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            id,
            command.default_binding,
        );
        assert!(!clashes.any_clash(), "{id}: {clashes:?}");
    }
}
