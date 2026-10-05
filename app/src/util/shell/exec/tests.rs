//! Opening a shell through the window: `OpenExecSession` docks a shell panel
//! over that container, on the asking panel's context, and asking again for the
//! same container brings that panel back rather than opening a second.

use super::OpenExecSession;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::exec::ExecTarget;
use crate::ui::nav::NavTarget;
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::{TestAppContext, VisualTestContext};

#[gpui_kit::test]
async fn opening_a_shell_docks_one_panel_per_container(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connecting);
    });
    let window =
        cx.add_window(|window, cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    // Dispatched from focus inside the window, as the Pods panel's `s` is.
    window
        .update(&mut vcx, |main, window, cx| main.test_focus(window, cx))
        .unwrap();
    let target = ExecTarget {
        namespace: "shop".into(),
        pod: "web-1".into(),
        container: "app".into(),
    };
    let shells = |vcx: &mut VisualTestContext| {
        window
            .update(vcx, |main, _, _| {
                let WindowMode::Workspace { open_panels, .. } = &main.mode else {
                    return Vec::new();
                };
                open_panels
                    .iter()
                    .filter(|open| matches!(open.key.target, NavTarget::Exec(_)))
                    .map(|open| (open.key.context_name.clone(), open.key.target.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap()
    };

    for _ in 0..2 {
        let action = OpenExecSession {
            context_name: "demo".into(),
            target: target.clone(),
        };
        vcx.update(|window, cx| window.dispatch_action(Box::new(action), cx));
        vcx.run_until_parked();
    }

    assert_eq!(
        shells(&mut vcx),
        [("demo".to_string(), NavTarget::Exec(target))],
        "one shell panel, on the asking context"
    );
}
