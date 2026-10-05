//! `pod-detail-logs-keybinding` through a real window: `l` in a pod's detail
//! panel opens the Logs panel on that pod, and pressed again focuses that
//! same Logs panel rather than opening a second.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::SelectedPod;
use crate::ui::nav::{NavTarget, OpenedPanel};
use crate::util::shell::test_support::temp_workspace_path;
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, TestAppContext, VisualTestContext};

/// 2.2: the window turns the detail panel's `l` into the Logs panel, on the
/// pod's own context, with that pod selected.
#[gpui_kit::test]
async fn l_in_a_pod_detail_panel_opens_its_logs(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
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
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let pod = NavTarget::pod("staging", "web-1");
    vcx.update(|window, cx| main.update(cx, |main, cx| main.open_target(pod.clone(), window, cx)));
    vcx.run_until_parked();
    let detail = vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
            return None;
        };
        open_panels.iter().find_map(|open| match &open.panel {
            Some(OpenedPanel::PodDetail(panel)) => Some(panel.clone()),
            _ => None,
        })
    });
    let detail = detail.expect("the pod's detail panel");
    let focus_detail = |vcx: &mut VisualTestContext| {
        vcx.update(|window, cx| {
            use gpui_kit::Focusable as _;
            let handle = detail.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        });
        vcx.run_until_parked();
    };
    vcx.update(|_, cx| {
        detail.update(cx, |panel, cx| {
            let loaded = serde_json::from_value(serde_json::json!({
                "apiVersion": "v1", "kind": "Pod",
                "metadata": { "name": "web-1", "namespace": "staging" },
                "spec": { "containers": [{ "name": "web" }, { "name": "proxy" }] },
            }))
            .expect("a pod");
            panel.test_set_loaded(loaded, cx);
        })
    });
    focus_detail(&mut vcx);

    vcx.simulate_keystrokes("l");
    vcx.run_until_parked();

    let logs_panels = |vcx: &mut VisualTestContext| {
        vcx.update(|_, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
                return Vec::new();
            };
            open_panels
                .iter()
                .filter(|open| open.key.target == NavTarget::Logs)
                .map(|open| open.key.context_name.clone())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        logs_panels(&mut vcx),
        ["demo"],
        "Logs opened on the pod's context"
    );
    let selected = vcx.update(|_, cx| cx.try_global::<SelectedPod>().and_then(|s| s.0.clone()));
    let selected = selected.expect("the pod is selected");
    assert_eq!(
        (selected.namespace.as_str(), selected.name.as_str()),
        ("staging", "web-1")
    );
    assert_eq!(
        selected.containers,
        ["web", "proxy"],
        "Logs starts on the first"
    );

    focus_detail(&mut vcx);
    vcx.simulate_keystrokes("l");
    vcx.run_until_parked();
    assert_eq!(
        logs_panels(&mut vcx).len(),
        1,
        "the same Logs panel, focused"
    );
}
