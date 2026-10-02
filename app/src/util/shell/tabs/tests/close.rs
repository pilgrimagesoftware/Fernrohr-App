//! `tab-close-buttons`: each close control closes the panel it belongs to,
//! clicked on the drawn control in a window hosted the way the app hosts one
//! (inside `Root`, where the close route finds the window), and the last panel
//! closes to the picker.

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

/// gpui-kit 0.7's per-tab close control (`dock/tab_panel.rs`).
const TAB_CLOSE: &str = "dock-tab-close-button";

struct Harness {
    vcx: VisualTestContext,
    main: Entity<MainWindow>,
}

impl Harness {
    /// A workspace window on `kind-dev` holding only Pods, inside `Root`.
    fn new(cx: &mut TestAppContext) -> Self {
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

    fn dock(&mut self) -> Entity<DockArea> {
        self.vcx.update(|_, cx| match &self.main.read(cx).mode {
            WindowMode::Workspace { dock_area, .. } => dock_area.clone(),
            WindowMode::Picker(_) => panic!("the window is in workspace mode"),
        })
    }

    /// The window's only panel at the start: Pods.
    fn pods(&mut self) -> PanelId {
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
    fn open_list(&mut self, plural: &str) -> PanelId {
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

    fn in_dock(&mut self, id: PanelId) -> bool {
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

    fn focus(&mut self, id: PanelId) {
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

    /// Clicks the title bar close control of `id`'s panel.
    fn click_title_close(&mut self, id: PanelId) {
        let selector: &'static str = format!("panel-close-{}", id.as_u64()).leak();
        self.click(selector);
    }

    fn is_picker(&mut self) -> bool {
        self.vcx
            .update(|_, cx| matches!(self.main.read(cx).mode, WindowMode::Picker(_)))
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

    h.click(TAB_CLOSE);

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
fn stacked(h: &mut Harness) -> (PanelId, PanelId) {
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

/// 2.1: the window's only panel closes from its close control, and the
/// window returns to the picker.
#[gpui_kit::test]
async fn the_last_panels_close_control_returns_to_the_picker(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let pods = h.pods();

    h.click_title_close(pods);

    assert!(h.is_picker(), "the window shows the picker");
}

/// 2.1: `Cmd-W` on the window's only panel closes it too, to the picker, and
/// keeps the window.
#[gpui_kit::test]
async fn cmd_w_on_the_last_panel_returns_to_the_picker(cx: &mut TestAppContext) {
    let mut h = Harness::new(cx);
    let pods = h.pods();
    h.focus(pods);

    press(&mut h.vcx, CLOSE_KEY);

    assert!(h.is_picker(), "the window shows the picker, still open");
}
