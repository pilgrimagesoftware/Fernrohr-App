//! Window-level tests for `panel-focus-navigation`, driven by real keystrokes
//! through the app's own bindings (`init`): Focus Next / Previous Panel cycling
//! the window's panels, a filter field passing `cmd-]` through, and an opened
//! or re-shown panel taking focus. The stop order itself is tested in
//! `ui::panel::focus`.

use crate::command::{CommandRegistry, MenuSlot};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::panel_title::PanelScope;
use crate::ui::placeholder::PlaceholderPanel;
use crate::util::shell::test_support::{Handles, focused, handles, press, workspace};
use crate::util::shell::{MainWindow, NavTarget, OpenedPanel, WindowMode, register_commands};
use gpui_kit::component::dock::{DockLayout, DockPlacement, panel_handle};
use gpui_kit::{
    AppContext as _, Entity, FocusHandle, Focusable as _, TestAppContext, VisualTestContext,
    WindowHandle,
};
use kube::core::GroupVersionKind;

/// Puts a panel in the window's right dock, so the dock has two groups -
/// every panel the window opens itself lands as a tab in the center group.
fn add_right_panel(window: WindowHandle<MainWindow>, cx: &mut VisualTestContext) -> FocusHandle {
    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let kind = DiscoveredKind {
                gvk: GroupVersionKind::gvk("", "v1", "Fern"),
                plural: "Ferns".into(),
                namespaced: true,
                verbs: Default::default(),
            };
            let panel: Entity<PlaceholderPanel> = cx.new(|cx| {
                PlaceholderPanel::with_namespaces(
                    kind.clone(),
                    PanelScope::new(NavTarget::Kind(kind), "kind-dev".into()),
                    cx.new(|_| NamespaceList::empty()),
                    cx,
                )
            });
            let handle = panel.read(cx).focus_handle(cx);
            dock_area.update(cx, |area, cx| {
                let layout = DockLayout::tabs().panel_view(panel_handle(panel), cx);
                area.set_dock(DockPlacement::Right, layout, window, cx);
                if !area.is_dock_open(DockPlacement::Right) {
                    area.toggle_dock(DockPlacement::Right, window, cx);
                }
            });
            handle
        })
        .unwrap()
}

/// Both commands are global, in the Navigate menu, and their default keys are
/// no other registered command's.
#[test]
fn the_focus_commands_are_global_navigate_items_with_their_own_keys() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);
    for id in ["panel.focus_next", "panel.focus_previous"] {
        let command = registry.get(id).expect("registered");
        assert!(
            registry.available(&[]).iter().any(|c| c.id == id),
            "{id} is offered with no panel focused"
        );
        assert_eq!(command.menu, Some(MenuSlot::Navigate));
        let shared: Vec<&str> = registry
            .iter()
            .filter(|other| {
                other.id != id
                    && !other.default_binding.is_empty()
                    && other.default_binding == command.default_binding
            })
            .map(|other| other.id)
            .collect();
        assert!(
            shared.is_empty(),
            "{id}'s default {:?} is also {shared:?}'s",
            command.default_binding
        );
    }
}

/// `cmd-]` steps Resource → the center group's panel → the right dock's panel,
/// then wraps back to Resource; `cmd-[` steps back.
#[gpui_kit::test]
async fn cmd_brackets_cycle_through_the_windows_panels(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let right = add_right_panel(window, &mut vcx);
    vcx.run_until_parked();
    let Handles { resource, pods } = handles(window, &mut vcx);
    assert!(focused(window, &resource, &mut vcx), "starts on Resource");

    press(&mut vcx, "cmd-]");
    assert!(
        focused(window, &pods, &mut vcx),
        "next: the center group's panel"
    );
    press(&mut vcx, "cmd-]");
    assert!(
        focused(window, &right, &mut vcx),
        "next: the right dock's panel"
    );
    press(&mut vcx, "cmd-]");
    assert!(
        focused(window, &resource, &mut vcx),
        "next wraps back to Resource"
    );
    press(&mut vcx, "cmd-[");
    assert!(
        focused(window, &right, &mut vcx),
        "previous wraps to the last panel"
    );
    press(&mut vcx, "cmd-[");
    assert!(focused(window, &pods, &mut vcx), "previous steps back");
}

/// The Resource filter is a text input, whose own Indent binding uses the same
/// key on macOS. A single-line input doesn't handle Indent, so `cmd-]` still
/// moves focus on - and counts the filter as the Resource panel's stop.
#[gpui_kit::test]
async fn a_focused_filter_field_passes_cmd_bracket_to_the_panel_command(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let Handles { resource, pods } = handles(window, &mut vcx);
    // The filter field is drawn only once kinds have loaded.
    window
        .update(&mut vcx, |main_window, _window, cx| {
            let kind = DiscoveredKind {
                gvk: GroupVersionKind::gvk("", "v1", "Pod"),
                plural: "pods".into(),
                namespaced: true,
                verbs: Default::default(),
            };
            main_window
                .test_resource_panel()
                .expect("a workspace window")
                .update(cx, |panel, cx| panel.test_show_kinds(vec![kind], cx));
        })
        .unwrap();
    vcx.run_until_parked();

    press(&mut vcx, "/");
    let list_focused = window
        .update(&mut vcx, |main_window, window, cx| {
            main_window
                .test_resource_panel()
                .expect("a workspace window")
                .read(cx)
                .is_list_focused(window)
        })
        .unwrap();
    assert!(
        !list_focused && focused(window, &resource, &mut vcx),
        "`/` moved focus into the filter field, inside the Resource panel"
    );

    press(&mut vcx, "cmd-]");
    assert!(
        focused(window, &pods, &mut vcx),
        "the filter field let `cmd-]` through to Focus Next Panel"
    );
}

/// A panel opened from the keyboard takes focus - `d` in the Pods list leaves
/// focus in the pod's new detail panel - and asking for an already-open panel
/// (`cmd-1`, Show Pods) brings it back with focus.
#[gpui_kit::test]
async fn an_opened_or_reshown_panel_takes_focus(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let Handles { pods, .. } = handles(window, &mut vcx);
    vcx.update(|_window, cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })));
    });
    window
        .update(&mut vcx, |_, window, cx| pods.focus(window, cx))
        .unwrap();
    vcx.run_until_parked();

    press(&mut vcx, "d");
    let detail = window
        .update(&mut vcx, |main_window, _window, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            open_panels
                .iter()
                .find(|open| open.key.target == NavTarget::pod("default", "web-1"))
                .and_then(|open| match open.panel.clone() {
                    Some(OpenedPanel::PodDetail(panel)) => Some(panel.read(cx).focus_handle(cx)),
                    _ => None,
                })
                .expect("`d` opened the pod's detail panel")
        })
        .unwrap();
    assert!(
        focused(window, &detail, &mut vcx),
        "the new detail panel has focus"
    );
    assert!(
        !focused(window, &pods, &mut vcx),
        "and the Pods list doesn't"
    );

    press(&mut vcx, "cmd-1");
    assert!(
        focused(window, &pods, &mut vcx),
        "re-showing the open Pods panel focuses it"
    );
}
