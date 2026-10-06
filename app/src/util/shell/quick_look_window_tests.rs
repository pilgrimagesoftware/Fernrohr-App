//! `pod-quick-look` through the real window: the dock, its tab panels and the
//! app's keymap. Space on a pod selected in the window's Pods panel opens the
//! quick look, as the user presses it - not only in a bare `Root`.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::SelectedPod;
use crate::k8s::test_cluster::FakeCluster;
use crate::util::shell::test_support::{handles, press, temp_workspace_path};
use crate::util::shell::{MainWindow, init};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

/// A window on `demo`, whose cluster holds `shop/web-1`, with Pods shown,
/// focused and listing it.
fn window_with_pods(cx: &mut TestAppContext) -> VisualTestContext {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
    });
    let (cluster, client) = FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "pods",
        json!({ "apiVersion": "v1", "kind": "Pod",
            "metadata": { "name": "web-1", "namespace": "shop", "uid": "u1" },
            "spec": { "containers": [{ "name": "app", "image": "nginx" }] } }),
    );
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "demo", ConnectionState::Connected(client));
    });
    let window =
        cx.add_window(|window, cx| MainWindow::test_workspace(vec!["demo".into()], window, cx));
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut vcx, |main, window, cx| main.test_focus(window, cx))
        .unwrap();
    press(&mut vcx, "cmd-1");
    let pods = handles(window, &mut vcx).pods;
    assert!(
        window
            .update(&mut vcx, |_, window, cx| pods.contains_focused(window, cx))
            .unwrap(),
        "Pods is shown and focused"
    );
    for _ in 0..400 {
        vcx.run_until_parked();
        if vcx.debug_bounds("pod-cell-0-0").is_some() {
            return vcx;
        }
        vcx.update(|window, cx| window.render_frame(cx));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("never listed web-1");
}

fn quick_look_drawn(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find("pod-quick-look").is_some()
    })
}

fn selected(vcx: &mut VisualTestContext) -> Option<String> {
    vcx.update(|_, cx| {
        cx.try_global::<SelectedPod>()
            .and_then(|selected| selected.0.clone())
            .map(|pod| pod.name)
    })
}

/// Selecting from the keyboard, then Space.
#[gpui_kit::test]
async fn space_after_down_opens_the_quick_look_in_the_window(cx: &mut TestAppContext) {
    let mut vcx = window_with_pods(cx);
    press(&mut vcx, "down");
    assert_eq!(selected(&mut vcx).as_deref(), Some("web-1"));
    press(&mut vcx, "space");
    assert!(quick_look_drawn(&mut vcx), "Space opened the quick look");
}

/// Selecting with a click, then Space - the route a mouse user takes first.
#[gpui_kit::test]
async fn space_after_a_click_opens_the_quick_look_in_the_window(cx: &mut TestAppContext) {
    let mut vcx = window_with_pods(cx);
    let row = vcx.debug_bounds("pod-cell-0-0").unwrap().center();
    vcx.simulate_click(row, gpui_kit::Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(selected(&mut vcx).as_deref(), Some("web-1"));
    press(&mut vcx, "space");
    assert!(quick_look_drawn(&mut vcx), "Space opened the quick look");
}
