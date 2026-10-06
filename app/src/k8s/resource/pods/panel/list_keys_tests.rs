//! `standard-resource-panels` 5.2 for the Pods table, in a real window with the
//! app's own keymap (`util::shell::init`): with the panel focused and no pod
//! selected, Down selects the first visible pod and Up the last.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::namespaces::NamespaceList;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::test_support::pod;
use crate::k8s::resource::pods::{PodsPanel, SelectedPod};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Focusable as _, Keystroke, TestAppContext, VisualTestContext};
use kube_runtime::watcher;

const CONTEXT: &str = "list-keys-pods";

fn temp_path() -> std::path::PathBuf {
    crate::util::test_paths::temp_path("pods-list-keys")
}

/// A connected Pods panel listing `web-1`, `web-2` and `web-3`, focused as a
/// whole - nothing selected - in a `Root` with the app's keymap.
fn focused_pods(cx: &mut TestAppContext) -> VisualTestContext {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, temp_path(), &temp_path());
    });
    let client = {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            CONTEXT,
            ConnectionState::Connected(client.clone()),
        )
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| {
            let connection =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
            let namespaces = cx.new(|_| NamespaceList::empty());
            PodsPanel::with_connection(
                PanelScope::new(NavTarget::pods(), CONTEXT.to_string()),
                connection,
                namespaces,
                cx,
            )
        });
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        let table = panel.read(cx).table.clone();
        table.update(cx, |table, cx| {
            table.apply(watcher::Event::Init);
            for (uid, name) in [("u1", "web-1"), ("u2", "web-2"), ("u3", "web-3")] {
                table.apply(watcher::Event::InitApply(pod(uid, name)));
            }
            table.apply(watcher::Event::InitDone);
            cx.notify();
        });
        panel.read(cx).focus_handle(cx).focus(window, cx);
    });
    vcx.run_until_parked();
    vcx
}

fn press(vcx: &mut VisualTestContext, key: &str) {
    vcx.simulate_keystrokes(&Keystroke::parse(key).expect("valid").unparse());
    vcx.run_until_parked();
}

fn selected(vcx: &mut VisualTestContext) -> Option<String> {
    vcx.update(|_, cx| {
        cx.try_global::<SelectedPod>()
            .and_then(|selected| selected.0.clone())
            .map(|pod| pod.name)
    })
}

#[gpui_kit::test]
async fn down_on_a_freshly_focused_pods_table_selects_the_first_pod(cx: &mut TestAppContext) {
    let mut vcx = focused_pods(cx);
    assert_eq!(selected(&mut vcx), None);
    press(&mut vcx, "down");
    assert_eq!(selected(&mut vcx).as_deref(), Some("web-1"));
}

#[gpui_kit::test]
async fn up_on_a_freshly_focused_pods_table_selects_the_last_pod(cx: &mut TestAppContext) {
    let mut vcx = focused_pods(cx);
    press(&mut vcx, "up");
    assert_eq!(selected(&mut vcx).as_deref(), Some("web-3"));
}
