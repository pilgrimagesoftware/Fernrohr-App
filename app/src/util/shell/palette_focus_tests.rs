//! App#410: ⌘⇧P opens the command palette whatever panel has focus - a Logs
//! panel and a detail panel included, not only list panels. Driven through a
//! real window with the app's keymap.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::{NavTarget, OpenedPanel, PodRef};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, TOGGLE_PALETTE_DEFAULT_BINDING, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{
    AppContext as _, Entity, Focusable as _, Keystroke, TestAppContext, VisualTestContext,
};

/// A workspace window on context `demo`, with the app's keymap.
fn workspace(cx: &mut TestAppContext) -> (Entity<MainWindow>, VisualTestContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
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
    let vcx = VisualTestContext::from_window(window.into(), cx);
    (main, vcx)
}

/// Opens `target`, focuses the panel `pick` finds among the open ones, then
/// presses ⌘⇧P: whether the palette opened.
fn palette_opens_from(
    main: &Entity<MainWindow>,
    vcx: &mut VisualTestContext,
    target: NavTarget,
    pick: impl Fn(&OpenedPanel, &gpui_kit::App) -> Option<gpui_kit::FocusHandle>,
    what: &str,
) -> bool {
    vcx.update(|window, cx| main.update(cx, |main, cx| main.open_target(target, window, cx)));
    vcx.run_until_parked();
    let focus = vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return None;
        };
        open_panels
            .iter()
            .find_map(|open| open.panel.as_ref().and_then(|panel| pick(panel, cx)))
    });
    let focus = focus.unwrap_or_else(|| panic!("{what} is open"));
    vcx.update(|window, cx| window.focus(&focus, cx));
    vcx.run_until_parked();
    assert!(
        vcx.update(|window, _| focus.is_focused(window)),
        "{what} has focus"
    );

    // On macOS the View menu's item takes ⌘⇧P first, and is enabled only
    // while the action is available from the focused element - the route a
    // simulated keystroke doesn't take.
    assert!(
        vcx.update(
            |window, cx| window.is_action_available(&crate::util::shell::ToggleCommandPalette, cx)
        ),
        "the palette's menu item is enabled with {what} focused"
    );
    let key = Keystroke::parse(TOGGLE_PALETTE_DEFAULT_BINDING)
        .expect("a valid keystroke")
        .unparse();
    vcx.simulate_keystrokes(&key);
    vcx.run_until_parked();
    let opened = vcx.update(|window, cx| window.has_active_dialog(cx));
    // Closed before asserting, so a failure reports the panel rather than the
    // leak detector's complaint about the palette's model.
    vcx.update(|window, cx| window.close_all_dialogs(cx));
    vcx.run_until_parked();
    opened
}

#[gpui_kit::test]
async fn the_palette_opens_from_a_focused_logs_panel(cx: &mut TestAppContext) {
    let (main, mut vcx) = workspace(cx);
    let logs = NavTarget::PodLogs(PodRef {
        namespace: "staging".into(),
        name: "web-1".into(),
    });
    let opened = palette_opens_from(
        &main,
        &mut vcx,
        logs,
        |panel, cx| match panel {
            OpenedPanel::Logs(panel) => Some(panel.read(cx).focus_handle(cx)),
            _ => None,
        },
        "the Logs panel",
    );
    assert!(opened, "⌘⇧P opens the palette from a focused Logs panel");
}

#[gpui_kit::test]
async fn the_palette_opens_from_a_focused_pod_detail_panel(cx: &mut TestAppContext) {
    let (main, mut vcx) = workspace(cx);
    let opened = palette_opens_from(
        &main,
        &mut vcx,
        NavTarget::pod("staging", "web-1"),
        |panel, cx| match panel {
            OpenedPanel::PodDetail(panel) => Some(panel.read(cx).focus_handle(cx)),
            _ => None,
        },
        "the pod detail panel",
    );
    assert!(opened, "⌘⇧P opens the palette from a focused detail panel");
}
