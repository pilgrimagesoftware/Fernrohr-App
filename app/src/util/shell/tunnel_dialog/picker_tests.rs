//! #363: Set Tunnel for Context from the cluster picker, through the app's own
//! keymap. With no context selected it has nothing to apply to, so the window
//! doesn't offer it - the menu item shows disabled and the key opens nothing;
//! once a context is selected the key opens the tunnel dialog for it.

use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{
    MainWindow, SET_CONTEXT_TUNNEL_DEFAULT_BINDING, SetContextTunnel, WindowMode, init,
};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext};

/// A picker-mode window listing two contexts, none selected, with focus in
/// its search field - where the issue's report starts.
fn picker_window(cx: &mut TestAppContext) -> (Entity<MainWindow>, VisualTestContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_picker_window(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its MainWindow");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        let WindowMode::Picker(picker) = &main.read(cx).mode else {
            unreachable!("a picker-mode window")
        };
        let picker = picker.clone();
        picker.update(cx, |picker, cx| {
            picker.test_set_contexts(Ok(vec!["dev".into(), "staging".into()]));
            cx.notify();
        });
        let focus = picker.read(cx).command_focus_handle(cx);
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    (main, vcx)
}

fn available(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.is_action_available(&SetContextTunnel, cx))
}

fn dialog_open(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}

#[gpui_kit::test]
async fn with_nothing_selected_the_tunnel_command_is_unavailable(cx: &mut TestAppContext) {
    let (_main, mut vcx) = picker_window(cx);
    assert!(
        !available(&mut vcx),
        "no context selected: the menu item shows disabled"
    );
    press(&mut vcx, SET_CONTEXT_TUNNEL_DEFAULT_BINDING);
    assert!(!dialog_open(&mut vcx), "and the key opens nothing");
}

#[gpui_kit::test]
async fn selecting_a_context_makes_the_tunnel_key_open_its_dialog(cx: &mut TestAppContext) {
    let (_main, mut vcx) = picker_window(cx);
    press(&mut vcx, "down");
    assert!(
        available(&mut vcx),
        "a selected context: the command is offered"
    );
    press(&mut vcx, SET_CONTEXT_TUNNEL_DEFAULT_BINDING);
    assert!(dialog_open(&mut vcx), "and the key opens the tunnel dialog");
}
