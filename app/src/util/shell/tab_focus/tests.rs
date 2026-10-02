//! Focus when the focused tab closes (`panel-tab-focus-on-close` 2.x, 4.1-4.3),
//! driven through a real window: tabs opened with `d` from the Pods list, closed
//! with the platform's close key, and switched by clicking their drawn tabs.

use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::util::shell::test_support::{Handles, focused, handles, press, workspace};
use crate::util::shell::{MainWindow, NavTarget, WindowMode};
use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{FocusHandle, Modifiers, TestAppContext, VisualTestContext, WindowHandle};

/// `cmd-w` on macOS, `ctrl-w` elsewhere - the menu's platform key.
const CLOSE_KEY: &str = if cfg!(target_os = "macos") {
    "cmd-w"
} else {
    "ctrl-w"
};

struct Harness {
    window: WindowHandle<MainWindow>,
    vcx: VisualTestContext,
    handles: Handles,
}

fn harness(cx: &mut TestAppContext) -> Harness {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let handles = handles(window, &mut vcx);
    Harness {
        window,
        vcx,
        handles,
    }
}

/// Opens `pod`'s detail with `d` from the Pods list, as a user would: a new tab
/// in the Pods group, displayed and focused. Pods is shown first - clicked, once
/// it's a hidden tab - since a hidden panel's keys reach nothing.
fn describe(h: &mut Harness, pod: &str) -> (PanelId, FocusHandle) {
    let selection = PodSelection {
        namespace: "default".into(),
        name: pod.into(),
        containers: vec!["web".into()],
        context_name: "kind-dev".into(),
    };
    h.vcx
        .update(|_, cx| cx.set_global(SelectedPod(Some(selection))));
    let pods = h.handles.pods.clone();
    if focused(h.window, &h.handles.resource, &mut h.vcx) {
        h.window
            .update(&mut h.vcx, |_, window, cx| pods.focus(window, cx))
            .unwrap();
        h.vcx.run_until_parked();
    } else {
        click_tab(h, "Pods");
    }
    press(&mut h.vcx, "d");
    panel(h, &NavTarget::pod("default", pod))
}

fn panel(h: &mut Harness, target: &NavTarget) -> (PanelId, FocusHandle) {
    h.window
        .update(&mut h.vcx, |main_window, _, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let open = open_panels
                .iter()
                .find(|open| &open.key.target == target)
                .expect("the panel is open");
            let handle = open.panel.as_ref().expect("built here").focus_handle(cx);
            (open.id, handle)
        })
        .unwrap()
}

/// The panel the group holding `id` displays.
fn displayed_beside(h: &mut Harness, id: PanelId) -> Option<PanelId> {
    h.window
        .update(&mut h.vcx, |main_window, _, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                return None;
            };
            let area = dock_area.read(cx);
            let node = crate::ui::panel::tabs::group_of(area, id)?;
            crate::ui::panel::tabs::active_panel_of(area, node)
        })
        .unwrap()
}

fn click_tab(h: &mut Harness, title: &str) {
    h.vcx.update(|window, cx| window.render_frame(cx));
    let bounds = ["unfocused", "focused"]
        .into_iter()
        .find_map(|state| {
            let selector: &'static str = format!("panel-title-{title}-{state}").leak();
            h.vcx.debug_bounds(selector)
        })
        .unwrap_or_else(|| panic!("{title}'s tab is drawn"));
    h.vcx.simulate_click(bounds.center(), Modifiers::none());
    h.vcx.run_until_parked();
}

/// 4.1: closing the focused middle tab of three focuses the tab displayed in its
/// place, so its keys work at once.
#[gpui_kit::test]
async fn closing_the_focused_middle_tab_focuses_the_tab_in_its_place(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (first, first_focus) = describe(&mut h, "web-1");
    let (_, second_focus) = describe(&mut h, "web-2");
    click_tab(&mut h, "Pod: web-1");
    assert!(
        focused(h.window, &first_focus, &mut h.vcx),
        "the middle tab"
    );

    press(&mut h.vcx, CLOSE_KEY);
    let pods_id = panel(&mut h, &NavTarget::pods()).0;
    let shown = displayed_beside(&mut h, pods_id).expect("the group remains");
    assert_ne!(shown, first, "the closed tab is gone");
    let shown_focus = if shown == pods_id {
        h.handles.pods.clone()
    } else {
        second_focus
    };
    assert!(
        focused(h.window, &shown_focus, &mut h.vcx),
        "the tab displayed in its place has focus"
    );
}

/// 4.2: closing the focused last (rightmost) tab focuses its neighbour.
#[gpui_kit::test]
async fn closing_the_focused_last_tab_focuses_its_neighbour(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (_, detail_focus) = describe(&mut h, "web-1");
    assert!(focused(h.window, &detail_focus, &mut h.vcx));

    press(&mut h.vcx, CLOSE_KEY);
    let pods = h.handles.pods.clone();
    assert!(focused(h.window, &pods, &mut h.vcx), "Pods has focus");
    press(&mut h.vcx, "d");
    assert!(
        h.window
            .update(&mut h.vcx, |main_window, _, _| {
                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    return false;
                };
                open_panels
                    .iter()
                    .any(|open| open.key.target == NavTarget::pod("default", "web-1"))
            })
            .unwrap(),
        "and its own `d` works without a click"
    );
}

/// 4.2: closing the only panel of a split's group - the group goes with it -
/// focuses the panel in the group that remains.
#[gpui_kit::test]
async fn closing_a_groups_only_panel_focuses_another_group(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (detail, detail_focus) = describe(&mut h, "web-1");
    h.window
        .update(&mut h.vcx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            dock_area.update(cx, |area, cx| {
                let tree = area.layout(DockPlacement::Center).expect("a centre");
                let node = tree.find_panel_node(detail).expect("docked");
                let split = InsertTarget::Split {
                    node,
                    placement: gpui_kit::base::Placement::Bottom,
                    size: None,
                };
                area.move_panel(detail, split, window, cx);
            });
        })
        .unwrap();
    h.vcx.run_until_parked();
    h.window
        .update(&mut h.vcx, |_, window, cx| detail_focus.focus(window, cx))
        .unwrap();
    h.vcx.run_until_parked();

    press(&mut h.vcx, CLOSE_KEY);
    let pods = h.handles.pods.clone();
    assert!(
        focused(h.window, &pods, &mut h.vcx),
        "focus moved to the other group's panel"
    );
}

/// 4.3, the reported case: close the focused tab, then click another tab - it
/// takes focus at once.
#[gpui_kit::test]
async fn after_a_close_clicking_another_tab_focuses_it(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    describe(&mut h, "web-1");
    let (_, second_focus) = describe(&mut h, "web-2");
    assert!(focused(h.window, &second_focus, &mut h.vcx));
    press(&mut h.vcx, CLOSE_KEY);
    let resource = h.handles.resource.clone();
    h.window
        .update(&mut h.vcx, |_, window, cx| resource.focus(window, cx))
        .unwrap();
    h.vcx.run_until_parked();

    click_tab(&mut h, "Pods");
    let pods = h.handles.pods.clone();
    assert!(focused(h.window, &pods, &mut h.vcx), "the clicked tab");
}

/// 3.1, the reported "won't take focus until you switch tabs and back": clicking
/// the tab already displayed focuses its panel. gpui-base's `select_tab` skips
/// focus for the active tab; `panel_title::title_element` covers it.
#[gpui_kit::test]
async fn clicking_the_already_active_tab_focuses_its_panel(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (_, detail_focus) = describe(&mut h, "web-1");
    let resource = h.handles.resource.clone();
    h.window
        .update(&mut h.vcx, |_, window, cx| resource.focus(window, cx))
        .unwrap();
    h.vcx.run_until_parked();
    assert!(!focused(h.window, &detail_focus, &mut h.vcx));

    click_tab(&mut h, "Pod: web-1");
    assert!(
        focused(h.window, &detail_focus, &mut h.vcx),
        "the displayed tab's panel has focus"
    );
}
