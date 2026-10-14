//! `panel-tab-context-menu` in a real window: the tab commands close, pin and
//! move the focused tab (2.1-2.4); Open Tab Menu (`cmd-k m`) and a right-click
//! open the menu for the right tab, and its items run against that tab even
//! if it moved (2.5, 3.2, 3.3); pinned state is saved and restored (1.1, 1.2).

use super::super::test_support::{cluster_scoped_kind, crd_kind, press, workspace};
use super::super::{MainWindow, WindowMode};
use crate::ui::nav::{NavTarget, OpenMode};
use crate::ui::panel::tab_menu::{CloseOtherTabs, CloseTabsToRight, MoveTabToNewWindow, PinTab};
use gpui_kit::component::dock::PanelId;
use gpui_kit::{
    Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, VisualTestContext,
    WindowHandle,
};

fn deployments() -> crate::k8s::cluster::discovery::DiscoveredKind {
    crate::k8s::cluster::discovery::DiscoveredKind {
        gvk: kube::core::GroupVersionKind::gvk("apps", "v1", "Deployment"),
        plural: "deployments".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// A window whose one tab group holds, left to right, Pods, Ferns, Widgets
/// and Deployments, Deployments focused.
fn four_tabs(cx: &mut TestAppContext) -> (WindowHandle<MainWindow>, VisualTestContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    for kind in [crd_kind(), cluster_scoped_kind(), deployments()] {
        window
            .update(&mut vcx, |main, window, cx| {
                main.open_target_in(
                    NavTarget::Kind(kind),
                    None,
                    None,
                    Vec::new(),
                    OpenMode::Foreground,
                    window,
                    cx,
                );
            })
            .unwrap();
        vcx.run_until_parked();
    }
    (window, vcx)
}

/// The focused group's tabs, left to right, by the kind each shows.
fn tabs(window: WindowHandle<MainWindow>, vcx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(vcx, |main, window, cx| {
            let WindowMode::Workspace {
                dock_area,
                open_panels,
                ..
            } = &main.mode
            else {
                return Vec::new();
            };
            let area = dock_area.read(cx);
            let Some(group) = crate::ui::panel::tabs::focused_group(area, window, cx) else {
                return Vec::new();
            };
            group
                .panels
                .iter()
                .map(|id| {
                    open_panels
                        .iter()
                        .find(|open| open.id == *id)
                        .map(|open| match &open.key.target {
                            NavTarget::Kind(kind) => kind.gvk.kind.clone(),
                            other => format!("{other:?}"),
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap()
}

/// The dock id of the tab showing `kind`.
fn tab_of(window: WindowHandle<MainWindow>, vcx: &mut VisualTestContext, kind: &str) -> PanelId {
    window
        .update(vcx, |main, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.mode else {
                panic!("a workspace window");
            };
            open_panels
                .iter()
                .find(|open| matches!(&open.key.target, NavTarget::Kind(k) if k.gvk.kind == kind))
                .map(|open| open.id)
                .expect("that kind's tab")
        })
        .unwrap()
}

/// Focuses the tab showing `kind`.
fn focus(window: WindowHandle<MainWindow>, vcx: &mut VisualTestContext, kind: &str) {
    let id = tab_of(window, vcx, kind);
    window
        .update(vcx, |main, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                return;
            };
            let dock_area = dock_area.clone();
            dock_area.update(cx, |area, cx| area.select_panel(id, window, cx));
            if let Some(view) = dock_area.read(cx).panel(id) {
                window.focus(&view.focus_handle(cx), cx);
            }
        })
        .unwrap();
    vcx.run_until_parked();
}

fn dispatch(vcx: &mut VisualTestContext, action: Box<dyn gpui_kit::Action>) {
    vcx.update(|window, cx| window.dispatch_action(action, cx));
    vcx.run_until_parked();
}

/// 2.1: Close Other Tabs leaves only the focused tab; Close Tabs to the Right
/// closes only those right of it.
#[gpui_kit::test]
async fn close_others_and_close_to_the_right(cx: &mut TestAppContext) {
    let (window, mut vcx) = four_tabs(cx);
    focus(window, &mut vcx, "Fern");
    dispatch(&mut vcx, Box::new(CloseTabsToRight));
    assert_eq!(tabs(window, &mut vcx), ["Pod", "Fern"]);

    dispatch(&mut vcx, Box::new(CloseOtherTabs));
    assert_eq!(tabs(window, &mut vcx), ["Fern"]);
}

/// 2.2: pinning a tab moves it before its unpinned siblings, after any tab
/// already pinned; the bulk closes then spare it (2.4).
#[gpui_kit::test]
async fn pinned_tabs_sort_first_and_survive_bulk_closes(cx: &mut TestAppContext) {
    let (window, mut vcx) = four_tabs(cx);
    focus(window, &mut vcx, "Widget");
    dispatch(&mut vcx, Box::new(PinTab));
    assert_eq!(
        tabs(window, &mut vcx),
        ["Widget", "Pod", "Fern", "Deployment"]
    );
    focus(window, &mut vcx, "Deployment");
    dispatch(&mut vcx, Box::new(PinTab));
    assert_eq!(
        tabs(window, &mut vcx),
        ["Widget", "Deployment", "Pod", "Fern"]
    );

    // Close All Tabs is the dock's Close Group: the pinned tabs stay.
    dispatch(
        &mut vcx,
        Box::new(crate::ui::panel::arrange::ClosePanelGroup),
    );
    assert_eq!(tabs(window, &mut vcx), ["Widget", "Deployment"]);
    dispatch(&mut vcx, Box::new(CloseOtherTabs));
    assert_eq!(
        tabs(window, &mut vcx),
        ["Widget", "Deployment"],
        "nothing unpinned to close"
    );
}

/// 1.1, 1.2: a pinned tab saves `pinned`, an unpinned one saves nothing, and a
/// restored panel reads it back - an older state with none reads unpinned.
#[gpui_kit::test]
async fn pinned_state_is_saved_and_restored(cx: &mut TestAppContext) {
    let (window, mut vcx) = four_tabs(cx);
    focus(window, &mut vcx, "Fern");
    dispatch(&mut vcx, Box::new(PinTab));
    let state = window
        .update(&mut vcx, |main, _window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                panic!("a workspace window");
            };
            dock_area.read(cx).dump(cx)
        })
        .unwrap();
    let text = serde_json::to_string(&state).unwrap();
    assert_eq!(
        text.matches("\"pinned\":true").count(),
        1,
        "only the pinned tab saves it"
    );

    window
        .update(&mut vcx, |main, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                return;
            };
            dock_area.update(cx, |area, cx| area.load(state, window, cx).expect("loads"));
        })
        .unwrap();
    vcx.run_until_parked();
    let pinned: Vec<bool> = window
        .update(&mut vcx, |main, _window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                return Vec::new();
            };
            let area = dock_area.read(cx);
            crate::ui::panel::tabs::all_panels(area)
                .into_iter()
                .filter_map(|id| crate::ui::nav::opened_panel_for(area, id, cx))
                .map(|opened| opened.pinned(cx))
                .collect()
        })
        .unwrap();
    assert_eq!(pinned.iter().filter(|&&pinned| pinned).count(), 1);
    assert!(!crate::ui::panel_title::saved_pinned(
        &serde_json::json!({ "context_name": "dev" })
    ));
}

/// 2.3: Move to New Window takes the tab out of this window and opens a new
/// window whose only panel is that tab, its scope and pinned state intact.
#[gpui_kit::test]
async fn move_to_new_window_reopens_the_tab_alone(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |main, window, cx| {
            main.open_target_in(
                NavTarget::Kind(deployments()),
                None,
                None,
                vec!["team-a".into()],
                OpenMode::Foreground,
                window,
                cx,
            );
        })
        .unwrap();
    vcx.run_until_parked();
    dispatch(&mut vcx, Box::new(PinTab));
    let before = vcx.update(|_, cx| cx.windows().len());
    dispatch(&mut vcx, Box::new(MoveTabToNewWindow));
    vcx.run_until_parked();

    assert_eq!(tabs(window, &mut vcx), ["Pod"], "gone from this window");
    let windows = vcx.update(|_, cx| cx.windows());
    assert_eq!(windows.len(), before + 1, "a new window");
    let moved = windows
        .iter()
        .find_map(|handle| {
            handle
                .update(&mut vcx, |view, _window, cx| {
                    let root = view.downcast::<gpui_kit::component::Root>().ok()?;
                    let main = root.read(cx).view().clone().downcast::<MainWindow>().ok()?;
                    let views = main.read(cx).test_dock_views(cx);
                    let names: Vec<_> = views.iter().map(|view| view.panel_name(cx)).collect();
                    let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
                        return None;
                    };
                    let area = dock_area.read(cx);
                    let pinned = crate::ui::panel::tabs::all_panels(area)
                        .into_iter()
                        .filter_map(|id| crate::ui::nav::opened_panel_for(area, id, cx))
                        .map(|opened| opened.pinned(cx))
                        .collect::<Vec<_>>();
                    let dump = serde_json::to_string(&area.dump(cx)).ok()?;
                    Some((names, pinned, dump))
                })
                .ok()
                .flatten()
        })
        .expect("the new window's workspace");
    let (names, pinned, dump) = moved;
    assert_eq!(names, ["ObjectList"], "the tab, alone");
    assert_eq!(pinned, [true], "still pinned");
    assert!(dump.contains("\"team-a\""), "its namespace scope: {dump}");
    assert!(dump.contains("\"Deployment\""), "its kind: {dump}");
}

/// 2.5: with a tab focused, `cmd-k m` opens its menu; Down twice to Close
/// Other Tabs and Enter runs it on that tab.
#[gpui_kit::test]
async fn cmd_k_m_opens_the_focused_tabs_menu(cx: &mut TestAppContext) {
    let (window, mut vcx) = four_tabs(cx);
    focus(window, &mut vcx, "Widget");
    press(&mut vcx, "cmd-k");
    press(&mut vcx, "m");
    let open = window
        .update(&mut vcx, |main, _, _| main.tab_menu.is_open())
        .unwrap();
    assert!(open, "the menu is open");
    press(&mut vcx, "down");
    press(&mut vcx, "down");
    press(&mut vcx, "enter");
    assert_eq!(tabs(window, &mut vcx), ["Widget"]);
    let open = window
        .update(&mut vcx, |main, _, _| main.tab_menu.is_open())
        .unwrap();
    assert!(!open, "and closes");
}

/// 3.2: a right-click on a tab that isn't the focused one opens the menu for
/// that tab - Close Tab then closes it, not the focused one.
#[gpui_kit::test]
async fn a_right_click_targets_the_clicked_tab(cx: &mut TestAppContext) {
    let (window, mut vcx) = four_tabs(cx);
    focus(window, &mut vcx, "Deployment");
    let fern = tab_of(window, &mut vcx, "Fern");
    let entity = window
        .update(&mut vcx, |main, _window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main.mode else {
                panic!("a workspace window");
            };
            dock_area.read(cx).panel(fern).unwrap().view().entity_id()
        })
        .unwrap();
    vcx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let at = vcx.update(|_, cx| {
        crate::ui::panel::tab_menu::TabTitleBounds::of(entity, cx)
            .expect("the Fern tab's title was drawn")
            .center()
    });
    vcx.simulate_event(MouseDownEvent {
        position: at,
        modifiers: Modifiers::none(),
        button: MouseButton::Right,
        click_count: 1,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        position: at,
        modifiers: Modifiers::none(),
        button: MouseButton::Right,
        click_count: 1,
    });
    vcx.run_until_parked();
    let open = window
        .update(&mut vcx, |main, _, _| main.tab_menu.is_open())
        .unwrap();
    assert!(open, "the menu is open");
    // Close Tab, the first item.
    press(&mut vcx, "down");
    press(&mut vcx, "enter");
    let mut left = tabs(window, &mut vcx);
    left.sort();
    assert_eq!(
        left,
        ["Deployment", "Pod", "Widget"],
        "the clicked tab closed"
    );
}

/// 3.3: an item for a tab that has closed since its menu opened does
/// nothing, rather than act on whichever tab is focused now.
#[gpui_kit::test]
async fn an_item_for_a_closed_tab_does_nothing(cx: &mut TestAppContext) {
    let (window, mut vcx) = four_tabs(cx);
    focus(window, &mut vcx, "Fern");
    let fern = tab_of(window, &mut vcx, "Fern");
    window
        .update(&mut vcx, |main, window, cx| {
            main.open_tab_menu(
                fern,
                gpui_kit::point(gpui_kit::px(10.), gpui_kit::px(10.)),
                window,
                cx,
            );
        })
        .unwrap();
    // Fern closes while its menu is open - Cmd-W, say.
    dispatch(&mut vcx, Box::new(crate::ui::panel::tab_menu::CloseTab));
    focus(window, &mut vcx, "Widget");
    window
        .update(&mut vcx, |main, window, cx| {
            main.run_tab_item(
                fern,
                crate::ui::panel::tab_menu::TabMenuItem::CloseOthers,
                window,
                cx,
            );
        })
        .unwrap();
    vcx.run_until_parked();
    assert_eq!(
        tabs(window, &mut vcx),
        ["Pod", "Widget", "Deployment"],
        "nothing more closed"
    );
}

/// 4.1: every tab command is a palette command.
#[gpui_kit::test]
async fn the_tab_commands_are_registered(_cx: &mut TestAppContext) {
    let mut registry = crate::command::CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    for (id, title) in [
        ("tab.close", "Close Tab"),
        ("tab.close_others", "Close Other Tabs"),
        ("tab.close_right", "Close Tabs to the Right"),
        ("tab.move_to_new_window", "Move to New Window"),
        ("tab.pin", "Pin Tab"),
        ("tab.unpin", "Unpin Tab"),
        ("tab.menu", "Open Tab Menu"),
    ] {
        assert_eq!(
            registry.get(id).map(|command| command.title),
            Some(title),
            "{id}"
        );
    }
    assert_eq!(registry.get("tab.menu").unwrap().default_binding, "cmd-k m");
}

/// With `open-panel-new-instance`: a duplicated tab can be pinned like any
/// other, and moving it to a new window drops its instance number - alone
/// there, it's that window's first.
#[gpui_kit::test]
async fn a_duplicated_tab_pins_and_moves_as_the_new_windows_first(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    for mode in [OpenMode::Foreground, OpenMode::NewPanel] {
        window
            .update(&mut vcx, |main, window, cx| {
                main.open_target_in(
                    NavTarget::Kind(deployments()),
                    None,
                    None,
                    Vec::new(),
                    mode,
                    window,
                    cx,
                );
            })
            .unwrap();
        vcx.run_until_parked();
    }
    dispatch(&mut vcx, Box::new(PinTab));
    let tabs_now = tabs(window, &mut vcx);
    assert_eq!(
        tabs_now[0], "Deployment",
        "the duplicate sorts first, pinned"
    );
    dispatch(&mut vcx, Box::new(MoveTabToNewWindow));
    vcx.run_until_parked();
    let dumps: Vec<String> = vcx
        .update(|_, cx| cx.windows())
        .into_iter()
        .filter_map(|handle| {
            handle
                .update(&mut vcx, |view, _window, cx| {
                    let root = view.downcast::<gpui_kit::component::Root>().ok()?;
                    let main = root.read(cx).view().clone().downcast::<MainWindow>().ok()?;
                    let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
                        return None;
                    };
                    serde_json::to_string(&dock_area.read(cx).dump(cx)).ok()
                })
                .ok()
                .flatten()
        })
        .collect();
    let moved = dumps
        .iter()
        .find(|dump| dump.contains("\"Deployment\""))
        .expect("the new window holds the moved tab");
    assert!(
        !moved.contains("\"instance\""),
        "no number, alone there: {moved}"
    );
    assert!(moved.contains("\"pinned\":true"), "still pinned: {moved}");
}
