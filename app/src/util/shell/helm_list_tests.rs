//! helm-releases 3.1: the Helm Releases list opened from the keyboard alone,
//! through the command palette, in a connected window with the app's keymap.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, NavTarget, TOGGLE_PALETTE_DEFAULT_BINDING, WindowMode, init};
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{AppContext as _, Keystroke, TestAppContext, VisualTestContext};

#[gpui_kit::test]
async fn the_palette_opens_helm_releases(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
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
    let main = built.expect("the window built its MainWindow");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    let palette = Keystroke::parse(TOGGLE_PALETTE_DEFAULT_BINDING)
        .expect("a valid keystroke")
        .unparse();
    vcx.simulate_keystrokes(&palette);
    vcx.run_until_parked();
    assert!(
        vcx.update(|window, cx| window.has_active_dialog(cx)),
        "the palette opened"
    );
    vcx.simulate_input("Helm Releases");
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    let opened = vcx.update(|_, cx| match &main.read(cx).mode {
        WindowMode::Workspace { open_panels, .. } => open_panels
            .iter()
            .any(|open| open.key.target == NavTarget::HelmReleases),
        WindowMode::Picker(_) => false,
    });
    assert!(
        opened,
        "Enter on the palette's Helm Releases opened the list"
    );
}
