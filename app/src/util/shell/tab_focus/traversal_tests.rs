//! Tab traversal stays inside the focused panel (`0-tab-traversal`), in the
//! layout it was reported in: a horizontal split whose left side is split
//! vertically - Pods upper-left, a Services list lower-left - with a second
//! Services list on the right. The window is the app's own: `shell::init`'s
//! keymap, inside a `Root` (which owns Tab).

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_list::{ObjectListPanel, ObjectsTable};
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, OpenedPanel, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::component::dock::{DockPlacement, InsertTarget, panel_handle};
use gpui_kit::{
    AppContext as _, Entity, FocusHandle, Focusable as _, TestAppContext, VisualTestContext,
};
use kube::api::{DynamicObject, ObjectMeta};
use kube::core::GroupVersionKind;
use kube_runtime::watcher;

fn services() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
    }
}

fn service(name: &str) -> DynamicObject {
    DynamicObject {
        types: None,
        metadata: ObjectMeta {
            uid: Some(format!("uid-{name}")),
            name: Some(name.into()),
            namespace: Some("default".into()),
            ..Default::default()
        },
        data: serde_json::Value::Null,
    }
}

struct Harness {
    vcx: VisualTestContext,
    resource: FocusHandle,
    pods: FocusHandle,
    lower_left: FocusHandle,
    right: FocusHandle,
}

/// A Services list over two services, connected, with no watch.
fn list(client: kube::Client, cx: &mut gpui_kit::App) -> Entity<ObjectListPanel> {
    let objects = cx.new(|_| {
        let mut table = ObjectsTable::default();
        for name in ["api", "web"] {
            table.apply(watcher::Event::Apply(service(name)));
        }
        table
    });
    let scope = PanelScope::new(NavTarget::Kind(services()), "kind-dev".into());
    cx.new(|cx| ObjectListPanel::with_table(services(), scope, objects, client, cx))
}

fn harness(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    let client = {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let handles = window
        .update(cx, |_, window, cx| {
            main.update(cx, |main, cx| {
                let WindowMode::Workspace {
                    dock_area,
                    open_panels,
                    ..
                } = &main.mode
                else {
                    panic!("a connected window is in workspace mode")
                };
                let Some(OpenedPanel::Pods(pods)) = open_panels[0].panel.clone() else {
                    panic!("a new workspace opens on Pods")
                };
                let lower_left = list(client.clone(), cx);
                let right = list(client, cx);
                dock_area.update(cx, |area, cx| {
                    let tree = area.layout(DockPlacement::Center).expect("a centre");
                    let pods_group = tree
                        .find_panel_node(pods.entity_id().into())
                        .expect("docked");
                    for (panel, node, placement) in [
                        (&lower_left, pods_group, gpui_kit::base::Placement::Bottom),
                        (&right, pods_group, gpui_kit::base::Placement::Right),
                    ] {
                        area.add_panel_view(
                            panel_handle(panel.clone()),
                            DockPlacement::Center,
                            None,
                            window,
                            cx,
                        );
                        let node = if placement == gpui_kit::base::Placement::Right {
                            // Beside the whole left column, not just Pods.
                            area.layout(DockPlacement::Center)
                                .expect("a centre")
                                .root()
                                .id()
                        } else {
                            node
                        };
                        area.move_panel(
                            panel.entity_id().into(),
                            InsertTarget::Split {
                                node,
                                placement,
                                size: None,
                            },
                            window,
                            cx,
                        );
                    }
                });
                (
                    main.test_resource_panel()
                        .expect("a workspace")
                        .read(cx)
                        .focus_handle(),
                    pods.read(cx).focus_handle(cx),
                    lower_left.read(cx).focus_handle(cx),
                    right.read(cx).focus_handle(cx),
                )
            })
        })
        .unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let (resource, pods, lower_left, right) = handles;
    Harness {
        vcx,
        resource,
        pods,
        lower_left,
        right,
    }
}

fn holds(h: &mut Harness, handle: &FocusHandle) -> bool {
    h.vcx
        .update(|window, cx| handle.contains_focused(window, cx))
}

fn focus(h: &mut Harness, handle: &FocusHandle) {
    let handle = handle.clone();
    h.vcx.update(|window, cx| window.focus(&handle, cx));
    h.vcx.run_until_parked();
}

/// The report: from the upper-left panel, Tab and Shift-Tab never leave it.
#[gpui_kit::test]
async fn tab_stays_inside_the_upper_left_panel(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let pods = h.pods.clone();
    focus(&mut h, &pods);
    for n in 0..6 {
        press(&mut h.vcx, "tab");
        assert!(holds(&mut h, &pods), "Tab {n} stayed in Pods");
    }
    for n in 0..6 {
        press(&mut h.vcx, "shift-tab");
        assert!(holds(&mut h, &pods), "Shift-Tab {n} stayed in Pods");
    }
}

/// A list with a filter and a table: Tab cycles its controls - out of the table
/// and round to the filter again - and never reaches another panel.
#[gpui_kit::test]
async fn tab_cycles_a_lists_controls_and_stays_in_it(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let lower_left = h.lower_left.clone();
    focus(&mut h, &lower_left);
    press(&mut h.vcx, "/");
    let filter = h
        .vcx
        .update(|window, cx| window.focused(cx))
        .expect("`/` focused the filter");
    let mut back_on_the_filter = false;
    for n in 0..6 {
        press(&mut h.vcx, "tab");
        assert!(holds(&mut h, &lower_left), "Tab {n} stayed in the list");
        if n > 0 && h.vcx.update(|window, cx| window.focused(cx)) == Some(filter.clone()) {
            back_on_the_filter = true;
        }
    }
    assert!(back_on_the_filter, "Tab came round to the filter again");
}

/// The Resource panel is a panel too: Tab cycles its header buttons and filter
/// and never reaches the dock.
#[gpui_kit::test]
async fn tab_stays_inside_the_resource_panel(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let resource = h.resource.clone();
    focus(&mut h, &resource);
    for n in 0..8 {
        press(&mut h.vcx, "tab");
        assert!(
            holds(&mut h, &resource),
            "Tab {n} stayed in the Resource panel"
        );
    }
}

/// Panels are crossed with Focus Next Panel (cmd-]), which reaches the right one.
#[gpui_kit::test]
async fn focus_next_panel_reaches_the_right_panel(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (pods, right) = (h.pods.clone(), h.right.clone());
    focus(&mut h, &pods);
    for _ in 0..3 {
        if holds(&mut h, &right) {
            break;
        }
        press(
            &mut h.vcx,
            crate::ui::panel::focus::FOCUS_NEXT_DEFAULT_BINDING,
        );
    }
    assert!(holds(&mut h, &right), "cmd-] reached the right-hand panel");
}
