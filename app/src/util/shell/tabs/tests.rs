//! Window-level tests for the tab commands and `Cmd-W`, driven by real
//! keystrokes through the app's own bindings: Pods and a pod's detail as two
//! tabs of one group, the way `d` leaves them.

use super::{close_window_confirmation_body, losing_a_tunnel};

mod close;
mod drop;
mod icons;
use crate::command::{CommandRegistry, MenuSlot};
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::util::shell::test_support::{Handles, focused, handles, press, workspace};
use crate::util::shell::{MainWindow, NavTarget, OpenedPanel, WindowMode, register_commands};
use gpui_kit::component::dock::PanelId;
use gpui_kit::{FocusHandle, Focusable as _, TestAppContext, VisualTestContext, WindowHandle};

/// `cmd-w` on macOS, `ctrl-w` elsewhere - the menu's platform key.
const CLOSE_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-w"
} else {
    "ctrl-w"
};

/// A workspace with Pods and `web-1`'s detail as two tabs of the center
/// group, the detail displayed and focused - `d` from the Pods list.
fn pods_and_detail(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<MainWindow>,
    VisualTestContext,
    Handles,
    FocusHandle,
    PanelId,
) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let handles = handles(window, &mut vcx);
    vcx.update(|_window, cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })));
    });
    window
        .update(&mut vcx, |_, window, cx| handles.pods.focus(window, cx))
        .unwrap();
    vcx.run_until_parked();
    press(&mut vcx, "d");
    let (detail, detail_id) = window
        .update(&mut vcx, |main_window, _window, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let open = open_panels
                .iter()
                .find(|open| open.key.target == NavTarget::pod("default", "web-1"))
                .expect("`d` opened the pod's detail panel");
            let Some(OpenedPanel::PodDetail(panel)) = open.panel.clone() else {
                panic!("a pod detail panel")
            };
            (panel.read(cx).focus_handle(cx), open.id)
        })
        .unwrap();
    (window, vcx, handles, detail, detail_id)
}

fn in_dock(window: WindowHandle<MainWindow>, id: PanelId, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                return false;
            };
            dock_area.read(cx).panel(id).is_some()
        })
        .unwrap()
}

/// All eleven are global and in the palette; Next/Previous are Navigate items.
#[test]
fn the_tab_commands_are_registered_and_global() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let available: Vec<&str> = registry.available(&[]).iter().map(|c| c.id).collect();
    let ids = ["tab.next".to_string(), "tab.previous".to_string()]
        .into_iter()
        .chain((1..=9).map(|n| format!("tab.select_{n}")));
    for id in ids {
        assert!(available.contains(&id.as_str()), "{id} is offered globally");
    }
    for id in ["tab.next", "tab.previous"] {
        assert_eq!(
            registry.get(id).unwrap().menu,
            Some(MenuSlot::Navigate(crate::command::NavigateGroup::Tabs))
        );
    }
}

/// No two commands share a non-empty default binding in the same scope.
#[test]
fn no_two_commands_share_a_default_binding() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    let commands: Vec<_> = registry.iter().collect();
    for (ix, a) in commands.iter().enumerate() {
        for b in &commands[ix + 1..] {
            let same_scope = a.context == b.context;
            assert!(
                a.default_binding.is_empty()
                    || !same_scope
                    || a.default_binding != b.default_binding,
                "{} and {} both default to {:?}",
                a.id,
                b.id,
                a.default_binding
            );
        }
    }
}

/// `cmd-shift-]` / `cmd-shift-[` switch the focused group's tab, wrapping,
/// and the shown tab takes focus - the hidden tab is reachable.
#[gpui_kit::test]
async fn shifted_brackets_cycle_the_focused_groups_tabs(cx: &mut TestAppContext) {
    let (window, mut vcx, handles, detail, _) = pods_and_detail(cx);
    assert!(
        focused(window, &detail, &mut vcx),
        "starts on the detail tab"
    );

    press(&mut vcx, "cmd-shift-]");
    assert!(
        focused(window, &handles.pods, &mut vcx),
        "next wraps to Pods"
    );
    press(&mut vcx, "cmd-shift-]");
    assert!(focused(window, &detail, &mut vcx), "and on to the detail");
    press(&mut vcx, "cmd-shift-[");
    assert!(
        focused(window, &handles.pods, &mut vcx),
        "previous steps back"
    );
}

/// `ctrl-N` shows the Nth tab, `ctrl-9` the last, and a position past the end
/// changes nothing.
#[gpui_kit::test]
async fn ctrl_digits_select_tabs_and_nine_is_the_last(cx: &mut TestAppContext) {
    let (window, mut vcx, handles, detail, _) = pods_and_detail(cx);

    press(&mut vcx, "ctrl-1");
    assert!(
        focused(window, &handles.pods, &mut vcx),
        "ctrl-1: the first tab"
    );
    press(&mut vcx, "ctrl-9");
    assert!(focused(window, &detail, &mut vcx), "ctrl-9: the last tab");
    press(&mut vcx, "ctrl-5");
    assert!(
        focused(window, &detail, &mut vcx),
        "ctrl-5 of two: unchanged"
    );
}

/// From the Resource panel, the first group in
/// panel-focus order switches.
#[gpui_kit::test]
async fn from_the_resource_panel_the_first_group_switches(cx: &mut TestAppContext) {
    let (window, mut vcx, handles, _detail, _) = pods_and_detail(cx);
    window
        .update(&mut vcx, |_, window, cx| handles.resource.focus(window, cx))
        .unwrap();
    vcx.run_until_parked();

    press(&mut vcx, "cmd-shift-]");
    assert!(
        focused(window, &handles.pods, &mut vcx),
        "the center group switched from the detail to Pods"
    );
}

/// `Cmd-W` with a tab focused closes that tab; the window and the other tab
/// stay.
#[gpui_kit::test]
async fn cmd_w_closes_the_focused_tab_and_keeps_the_window(cx: &mut TestAppContext) {
    let (window, mut vcx, _handles, _detail, detail_id) = pods_and_detail(cx);

    press(&mut vcx, CLOSE_KEY);
    assert!(
        !in_dock(window, detail_id, &mut vcx),
        "the detail tab closed"
    );
    assert!(
        window.update(&mut vcx, |_, _, _| ()).is_ok(),
        "the window is still open"
    );
    assert!(
        in_dock(window, handles_pods_id(window, &mut vcx), &mut vcx),
        "and Pods is still there"
    );
}

fn handles_pods_id(window: WindowHandle<MainWindow>, cx: &mut VisualTestContext) -> PanelId {
    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            open_panels[0].id
        })
        .unwrap()
}

/// With focus in the Resource panel `ClosePanel` would reach no tab group;
/// `Cmd-W` focuses the displayed tab first, so it still closes it.
#[gpui_kit::test]
async fn cmd_w_from_the_resource_panel_closes_the_displayed_tab(cx: &mut TestAppContext) {
    let (window, mut vcx, handles, _detail, detail_id) = pods_and_detail(cx);
    window
        .update(&mut vcx, |_, window, cx| handles.resource.focus(window, cx))
        .unwrap();
    vcx.run_until_parked();

    press(&mut vcx, CLOSE_KEY);
    assert!(
        !in_dock(window, detail_id, &mut vcx),
        "the displayed (detail) tab closed"
    );
    assert!(
        window.update(&mut vcx, |_, _, _| ()).is_ok(),
        "the window stays open"
    );
}

/// A window with no tab to close - here the picker - closes on `Cmd-W`, with
/// no tunnel to confirm.
#[gpui_kit::test]
async fn cmd_w_with_no_tab_closes_the_window(cx: &mut TestAppContext) {
    // `workspace` sets the app up; the window under test is a second one, in
    // picker mode, so it has no dock at all.
    let _workspace = workspace(cx);
    let window = cx.add_window(MainWindow::test_picker_window);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |main_window, window, cx| {
            main_window.focus_handle.clone().focus(window, cx);
        })
        .unwrap();
    vcx.run_until_parked();

    press(&mut vcx, CLOSE_KEY);
    assert!(
        window.update(&mut vcx, |_, _, _| ()).is_err(),
        "the window closed"
    );
}

/// Only a context this window alone holds, with a live forward, loses its
/// tunnel when the window closes.
#[test]
fn only_sole_holders_with_a_live_forward_lose_a_tunnel() {
    let contexts = ["a", "b", "c"].map(String::from).to_vec();
    let holders = [1, 2, 1];
    let live = |name: &str| name != "c";
    assert_eq!(
        losing_a_tunnel(&contexts, &holders, live),
        vec!["a".to_string()]
    );
}

#[test]
fn the_confirmation_names_the_contexts() {
    assert_eq!(
        close_window_confirmation_body(&["prod".into()]),
        "The tunnel for prod will disconnect."
    );
    assert_eq!(
        close_window_confirmation_body(&["prod".into(), "staging".into()]),
        "The tunnels for prod, staging will disconnect."
    );
}
