//! `tab-close-buttons`: each close control closes the panel it belongs to,
//! clicked on the drawn control in a window hosted the way the app hosts one
//! (inside `Root`, where the close route finds the window); the last panel
//! closes with the window still connected; and the close control sits beside
//! its title.

// Named imports, for the reason `super`'s note gives.
use super::CLOSE_KEY;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::panel::tabs::{active_panel_of, group_of};
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, NavTarget, WindowMode, init};
use gpui_kit::component::Placement;
use gpui_kit::component::Root;
use gpui_kit::component::dock::{DockArea, InsertTarget, PanelId};
use gpui_kit::{
    AppContext as _, Entity, FocusHandle, Modifiers, MouseButton, TestAppContext, VisualTestContext,
};

pub(super) struct Harness {
    pub(super) vcx: VisualTestContext,
    main: Entity<MainWindow>,
}

impl Harness {
    /// A workspace window on `kind-dev` holding only Pods, inside `Root`.
    pub(super) fn new(cx: &mut TestAppContext) -> Self {
        cx.executor().allow_parking();
        let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::runtime::init(cx);
            init(cx, workspace, &keymap);
            ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
        });
        let mut built = None;
        let window = cx.add_window(|window, cx| {
            let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
            built = Some(main.clone());
            Root::new(main, window, cx)
        });
        let vcx = VisualTestContext::from_window(window.into(), cx);
        vcx.run_until_parked();
        Self {
            vcx,
            main: built.expect("the window built its view"),
        }
    }

    pub(super) fn dock(&mut self) -> Entity<DockArea> {
        self.vcx.update(|_, cx| match &self.main.read(cx).mode {
            WindowMode::Workspace { dock_area, .. } => dock_area.clone(),
            WindowMode::Picker(_) => panic!("the window is in workspace mode"),
        })
    }

    /// The window's only panel at the start: Pods.
    pub(super) fn pods(&mut self) -> PanelId {
        let dock = self.dock();
        self.vcx.update(|_, cx| {
            dock.read(cx)
                .layout(gpui_kit::component::dock::DockPlacement::Center)
                .and_then(|tree| tree.panels().next())
                .expect("a new workspace opens on Pods")
        })
    }

    /// Opens a list panel over the core kind `plural` the way the app does
    /// (`open_target`), so the window records it among its open panels.
    pub(super) fn open_list(&mut self, plural: &str) -> PanelId {
        let target = NavTarget::Kind(DiscoveredKind {
            gvk: kube::core::GroupVersionKind::gvk("", "v1", plural),
            plural: plural.into(),
            namespaced: true,
        });
        let main = self.main.clone();
        let id = self.vcx.update(|window, cx| {
            main.update(cx, |main, cx| {
                main.open_target(target.clone(), window, cx);
                let WindowMode::Workspace { open_panels, .. } = &main.mode else {
                    panic!("the window is in workspace mode")
                };
                open_panels
                    .iter()
                    .find(|open| open.key.target == target)
                    .expect("the list opened")
                    .id
            })
        });
        self.vcx.run_until_parked();
        id
    }

    pub(super) fn in_dock(&mut self, id: PanelId) -> bool {
        let dock = self.dock();
        self.vcx.update(|_, cx| dock.read(cx).panel(id).is_some())
    }

    fn focus_handle(&mut self, id: PanelId) -> FocusHandle {
        let dock = self.dock();
        self.vcx.update(|_, cx| {
            dock.read(cx)
                .panel(id)
                .expect("the panel is in the dock")
                .focus_handle(cx)
        })
    }

    pub(super) fn focus(&mut self, id: PanelId) {
        let handle = self.focus_handle(id);
        self.vcx.update(|window, cx| window.focus(&handle, cx));
        self.vcx.run_until_parked();
    }

    fn has_focus(&mut self, id: PanelId) -> bool {
        let handle = self.focus_handle(id);
        self.vcx
            .update(|window, cx| handle.contains_focused(window, cx))
    }

    fn click(&mut self, selector: &'static str) {
        let bounds = self
            .vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("`{selector}` is drawn"));
        self.vcx
            .simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::none());
        self.vcx
            .simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn close_selector(id: PanelId) -> &'static str {
        format!("panel-close-{}", id.as_u64()).leak()
    }

    /// Clicks the close control drawn beside `id`'s title.
    fn click_title_close(&mut self, id: PanelId) {
        self.click(Self::close_selector(id));
    }

    fn is_workspace(&mut self) -> bool {
        self.vcx
            .update(|_, cx| matches!(self.main.read(cx).mode, WindowMode::Workspace { .. }))
    }

    fn resource_focused(&mut self) -> bool {
        let main = self.main.clone();
        self.vcx.update(|window, cx| {
            main.read(cx)
                .test_resource_panel()
                .expect("a workspace window")
                .read(cx)
                .focus_handle()
                .contains_focused(window, cx)
        })
    }

    fn set_resource_collapsed(&mut self, collapsed: bool) {
        self.main.update(&mut self.vcx, |main, cx| {
            if let WindowMode::Workspace {
                resource_collapsed, ..
            } = &mut main.mode
            {
                *resource_collapsed = collapsed;
            }
            cx.notify();
        });
        self.vcx.run_until_parked();
    }

    fn resource_collapsed(&mut self) -> bool {
        self.vcx.update(|_, cx| match &self.main.read(cx).mode {
            WindowMode::Workspace {
                resource_collapsed, ..
            } => *resource_collapsed,
            WindowMode::Picker(_) => panic!("the window is in workspace mode"),
        })
    }

    /// After the last panel closed: still connected, the Resource panel
    /// focused and drawn, and the empty area naming the key to open a kind.
    fn assert_stayed_connected(&mut self) {
        assert!(
            self.is_workspace(),
            "the window stays connected, not the picker"
        );
        assert!(!self.resource_collapsed(), "the Resource panel is drawn");
        assert!(self.resource_focused(), "and has focus");
        assert!(
            self.vcx.debug_bounds("empty-dock-hint").is_some(),
            "the empty panel area names the key to open a kind"
        );
    }

    /// Asserts `id`'s close control is drawn immediately after its title, not
    /// at the far edge of its group.
    fn assert_close_beside_title(&mut self, id: PanelId, title: &str) {
        let close = self
            .vcx
            .debug_bounds(Self::close_selector(id))
            .expect("the close control is drawn");
        let title = ["focused", "unfocused"]
            .into_iter()
            .find_map(|state| {
                let selector: &'static str = format!("panel-title-{title}-{state}").leak();
                self.vcx.debug_bounds(selector)
            })
            .expect("the title is drawn");
        let gap = close.left() - title.right();
        assert!(
            gap >= gpui_kit::px(0.) && gap < gpui_kit::px(16.),
            "the close control sits right after the title: title ends at {:?}, close starts at {:?}",
            title.right(),
            close.left()
        );
        assert!(
            close.top() < title.bottom() && title.top() < close.bottom(),
            "on the title's line"
        );
    }
}

/// 1.1: Pods (displayed) and Services share a group; the close control on the
/// Services tab closes Services, and Pods stays the displayed tab.
#[gpui_kit::test]
async fn a_background_tabs_close_control_closes_that_tab(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let pods = h.pods();
    let services = h.open_list("services");
    let dock = h.dock();
    h.vcx
        .update(|window, cx| dock.update(cx, |area, cx| area.select_panel(pods, window, cx)));
    h.vcx.run_until_parked();

    h.click_title_close(services);

    assert!(!h.in_dock(services), "the Services tab closed");
    assert!(h.in_dock(pods), "Pods stays open");
    let active = h.vcx.update(|_, cx| {
        let area = dock.read(cx);
        group_of(area, pods).and_then(|group| active_panel_of(area, group))
    });
    assert_eq!(active, Some(pods), "and stays the displayed tab");
}

/// Two groups stacked, with Pods in the upper one and RoleBindings in the
/// lower one.
pub(super) fn stacked(h: &mut Harness) -> (PanelId, PanelId) {
    let pods = h.pods();
    let bindings = h.open_list("rolebindings");
    let dock = h.dock();
    h.vcx.update(|window, cx| {
        dock.update(cx, |area, cx| {
            // A move, not `split_at`: that inserts without detaching, so
            // it is for a panel not yet in the dock.
            let group = group_of(area, pods).expect("Pods' group");
            let target = InsertTarget::Split {
                node: group,
                placement: Placement::Bottom,
                size: None,
            };
            area.move_panel(bindings, target, window, cx);
        })
    });
    h.vcx.run_until_parked();
    (pods, bindings)
}

/// 1.2 and 1.3, the spec's scenario: Pods has focus in the upper group, and
/// the lower group's close closes RoleBindings - not the focused Pods - and
/// leaves focus on Pods.
#[gpui_kit::test]
async fn the_unfocused_groups_close_closes_its_own_panel(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let (pods, bindings) = stacked(&mut h);
    h.focus(pods);

    h.click_title_close(bindings);

    assert!(!h.in_dock(bindings), "RoleBindings closed");
    assert!(h.in_dock(pods), "Pods stays open");
    assert!(h.has_focus(pods), "with focus where it was");
}

/// 1.3's other half: closing the focused group's panel hands focus to a panel
/// still open, as `Cmd-W` does.
#[gpui_kit::test]
async fn closing_the_focused_panel_moves_focus_on(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let (pods, bindings) = stacked(&mut h);
    h.focus(bindings);

    h.click_title_close(bindings);

    assert!(!h.in_dock(bindings), "RoleBindings closed");
    assert!(h.has_focus(pods), "focus moved on to Pods");
}

/// 4.1: the window's only panel closes from its close control, and the window
/// stays connected with the Resource panel focused.
#[gpui_kit::test]
async fn the_last_panels_close_control_keeps_the_window_connected(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let pods = h.pods();
    h.focus(pods);

    h.click_title_close(pods);

    assert!(!h.in_dock(pods), "Pods closed");
    h.assert_stayed_connected();
}

/// 4.1: `Cmd-W` on the window's only panel closes it the same way.
#[gpui_kit::test]
async fn cmd_w_on_the_last_panel_keeps_the_window_connected(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let pods = h.pods();
    h.focus(pods);

    press(&mut h.vcx, CLOSE_KEY);

    assert!(!h.in_dock(pods), "Pods closed");
    h.assert_stayed_connected();
}

/// 4.1: with the Resource panel collapsed, closing the last panel expands it
/// so focus has somewhere to go.
#[gpui_kit::test]
async fn closing_the_last_panel_expands_a_collapsed_resource_panel(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let pods = h.pods();
    h.set_resource_collapsed(true);
    h.focus(pods);

    h.click_title_close(pods);

    h.assert_stayed_connected();
}

/// 4.2: a lone panel's close control is beside its title, where a tab's is.
#[gpui_kit::test]
async fn a_lone_panels_close_sits_beside_its_title(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let pods = h.pods();

    h.assert_close_beside_title(pods, "Pods");
}

/// 4.2, the spec's scenario: a group closed down to one tab still draws the
/// remaining panel's close next to its title, as it did in the tab strip.
#[gpui_kit::test]
async fn closing_down_to_one_tab_keeps_the_close_beside_the_title(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let pods = h.pods();
    let services = h.open_list("services");
    h.assert_close_beside_title(services, "Services");

    h.click_title_close(services);

    assert!(!h.in_dock(services), "Services closed");
    h.assert_close_beside_title(pods, "Pods");
}

/// A press on a tab's close control that then moves is not a tab drag: the
/// control's mouse-down stops before the tab's drag handler sees it.
#[gpui_kit::test]
async fn pressing_a_tabs_close_control_starts_no_drag(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let _pods = h.pods();
    let services = h.open_list("services");
    let close = h
        .vcx
        .debug_bounds(Harness::close_selector(services))
        .expect("the close control is drawn");
    let start = close.center();

    h.vcx
        .simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    for step in 1..=3 {
        let to = gpui_kit::point(
            start.x + gpui_kit::px(20. * step as f32),
            start.y + gpui_kit::px(40.),
        );
        h.vcx
            .simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    }
    let dragging = h.vcx.update(|_, cx| cx.has_active_drag());
    h.vcx
        .simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
    h.vcx.run_until_parked();

    assert!(!dragging, "no tab drag started from the close control");
}
