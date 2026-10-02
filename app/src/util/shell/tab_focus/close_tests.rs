//! `tab-close-buttons` 4.3: with the Resource panel collapsed and one tab group,
//! every close of the focused panel - by its close control or `Cmd-W`,
//! alternating - leaves a panel focused, and the focus commands work from a
//! window where only the root has focus.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::OpenedPanel;
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, EntityId, FocusHandle, Modifiers, TestAppContext, VisualTestContext,
};

const CLOSE_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-w"
} else {
    "ctrl-w"
};

struct Harness {
    vcx: VisualTestContext,
    main: Entity<MainWindow>,
}

fn harness(cx: &mut TestAppContext) -> Harness {
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
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    // The key window, as the OS makes the window a keystroke reaches.
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    Harness { vcx, main }
}

/// Every open panel the window built: its entity, for the close control's
/// selector, and its focus handle.
fn panels(h: &mut Harness) -> Vec<(EntityId, FocusHandle)> {
    h.vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &h.main.read(cx).mode else {
            return Vec::new();
        };
        open_panels
            .iter()
            .filter_map(|open| open.panel.as_ref())
            .map(|panel| {
                let id = match panel {
                    OpenedPanel::Pods(panel) => panel.entity_id(),
                    OpenedPanel::PodDetail(panel) => panel.entity_id(),
                    OpenedPanel::ObjectList(panel) => panel.entity_id(),
                    OpenedPanel::Events(panel) => panel.entity_id(),
                    OpenedPanel::ObjectDetail(panel) => panel.entity_id(),
                    OpenedPanel::Logs(panel) => panel.entity_id(),
                    OpenedPanel::Placeholder(panel) => panel.entity_id(),
                };
                (id, panel.focus_handle(cx))
            })
            .collect()
    })
}

/// The open panel that has focus, if any.
fn focused_panel(h: &mut Harness) -> Option<EntityId> {
    let panels = panels(h);
    h.vcx.update(|window, cx| {
        panels
            .iter()
            .find(|(_, handle)| handle.contains_focused(window, cx))
            .map(|(id, _)| *id)
    })
}

fn describe(h: &mut Harness, pod: &str) {
    h.vcx.update(|_, cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: pod.into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })))
    });
    // Show Pods (cmd-1), then describe from it.
    press(&mut h.vcx, "cmd-1");
    press(&mut h.vcx, "d");
}

fn click_close(h: &mut Harness, panel: EntityId) {
    h.vcx.update(|window, cx| window.render_frame(cx));
    let selector: &'static str = format!("panel-close-{panel}").leak();
    let bounds = h
        .vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    h.vcx.simulate_click(bounds.center(), Modifiers::none());
    h.vcx.run_until_parked();
}

/// The report: closing the focused tab, close after close, alternating the
/// close control and `Cmd-W`, always leaves a panel focused. `restored` drops the
/// window's typed handles first, so each panel is known only by its dock id - as
/// it is for every panel a window restored from its saved layout.
fn close_repeatedly(cx: &mut TestAppContext, restored: bool) {
    let mut h = harness(cx);
    press(
        &mut h.vcx,
        crate::ui::resource_panel::TOGGLE_DEFAULT_BINDING,
    );
    for pod in ["web-1", "web-2", "web-3", "web-4"] {
        describe(&mut h, pod);
    }
    let all = panels(&mut h);
    assert_eq!(all.len(), 5, "Pods and four details, one group");
    if restored {
        h.vcx.update(|_, cx| {
            h.main.update(cx, |main, _| {
                if let WindowMode::Workspace { open_panels, .. } = &mut main.mode {
                    for open in open_panels.iter_mut() {
                        open.panel = None;
                    }
                }
            })
        });
    }
    let focused = |h: &mut Harness| {
        h.vcx.update(|window, cx| {
            all.iter()
                .find(|(_, handle)| handle.contains_focused(window, cx))
                .map(|(id, _)| *id)
        })
    };

    for n in 0..4 {
        let closing = focused(&mut h).unwrap_or_else(|| panic!("close {n}: a panel has focus"));
        let how = if n % 2 == 0 { "close control" } else { "Cmd-W" };
        if n % 2 == 0 {
            click_close(&mut h, closing);
        } else {
            press(&mut h.vcx, CLOSE_KEY);
        }
        let after = focused(&mut h);
        assert!(
            after.is_some(),
            "after close {n} ({how}), a panel has focus"
        );
        assert_ne!(
            after,
            Some(closing),
            "close {n} ({how}) closed the focused panel"
        );
    }
}

#[gpui_kit::test]
async fn every_close_of_the_focused_panel_leaves_a_panel_focused(cx: &mut TestAppContext) {
    close_repeatedly(cx, false);
}

#[gpui_kit::test]
async fn every_close_of_a_restored_focused_panel_leaves_a_panel_focused(cx: &mut TestAppContext) {
    close_repeatedly(cx, true);
}

/// With only the window root focused, Focus Next Panel and Focus Resources
/// still find a panel - Focus Resources expanding the collapsed panel first.
#[gpui_kit::test]
async fn the_focus_commands_work_from_a_root_focused_window(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(
        &mut h.vcx,
        crate::ui::resource_panel::TOGGLE_DEFAULT_BINDING,
    );
    let root = h.vcx.update(|_, cx| h.main.read(cx).focus_handle.clone());
    let focus_root = |h: &mut Harness| {
        let root = root.clone();
        h.vcx.update(|window, cx| window.focus(&root, cx));
        h.vcx.run_until_parked();
    };

    focus_root(&mut h);
    press(
        &mut h.vcx,
        crate::ui::panel::focus::FOCUS_NEXT_DEFAULT_BINDING,
    );
    assert!(focused_panel(&mut h).is_some(), "Cmd-] focused a panel");

    focus_root(&mut h);
    press(&mut h.vcx, "cmd-0");
    let (side, collapsed) = h
        .vcx
        .update(|_, cx| h.main.read(cx).test_resource_layout().unwrap());
    let _ = side;
    assert!(!collapsed, "Cmd-0 expanded the Resource panel");
    let resource = h.vcx.update(|_, cx| {
        h.main
            .read(cx)
            .test_resource_panel()
            .unwrap()
            .read(cx)
            .focus_handle()
    });
    assert!(
        h.vcx
            .update(|window, cx| resource.contains_focused(window, cx))
    );
}

/// With nothing focused at all, Focus Next Panel and Focus Resources still find a
/// panel.
#[gpui_kit::test]
async fn the_focus_commands_work_from_a_window_with_nothing_focused(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    press(
        &mut h.vcx,
        crate::ui::resource_panel::TOGGLE_DEFAULT_BINDING,
    );
    let blur = |h: &mut Harness| {
        h.vcx.update(|window, cx| window.blur(cx));
        h.vcx.run_until_parked();
        assert!(h.vcx.update(|window, cx| window.focused(cx)).is_none());
    };

    blur(&mut h);
    press(
        &mut h.vcx,
        crate::ui::panel::focus::FOCUS_NEXT_DEFAULT_BINDING,
    );
    assert!(focused_panel(&mut h).is_some(), "Cmd-] focused a panel");

    blur(&mut h);
    press(&mut h.vcx, "cmd-0");
    let resource = h.vcx.update(|_, cx| {
        h.main
            .read(cx)
            .test_resource_panel()
            .unwrap()
            .read(cx)
            .focus_handle()
    });
    assert!(
        h.vcx
            .update(|window, cx| resource.contains_focused(window, cx)),
        "Cmd-0 expanded and focused the Resource panel"
    );
}
