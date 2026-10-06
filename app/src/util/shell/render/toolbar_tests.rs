//! `toolbar-layout-with-gpui-kit` 3.1 through a real window: the top bar is a
//! gpui-kit toolbar with the app icon and name, and no context chips - the
//! contexts are the status bar's capsules, at the bottom.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, init};
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};

#[gpui_kit::test]
async fn the_toolbar_shows_the_icon_and_name_and_no_context_chips(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
        Root::new(main, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    vcx.update(|window, cx| window.render_frame(cx));

    let height = vcx.update(|window, _| window.bounds().size.height);
    let icon = vcx
        .debug_bounds("window-toolbar-icon")
        .expect("the icon is drawn");
    let name = vcx
        .debug_bounds("window-toolbar-name")
        .expect("the name is drawn");
    for (what, drawn) in [("icon", icon), ("name", name)] {
        assert!(
            drawn.bottom() < height * 0.1,
            "the {what} is in the top bar: {drawn:?}"
        );
    }
    let chip = vcx
        .debug_bounds("status-item-kind-dev")
        .expect("the context's capsule is drawn");
    assert!(
        chip.top() > name.bottom() && chip.top() > height * 0.9,
        "the context is at the bottom, not in the toolbar: {chip:?}"
    );
}
