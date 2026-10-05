//! `panel-tab-focus-on-close` 3.3: every panel type, opened the way a user opens
//! it, takes its own keys at once - no click and no tab switch first. Each test
//! opens the panel, then presses one of that panel's scoped keys and checks the
//! panel acted on it.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_list::OpenListedObject;
use crate::k8s::resource::pod_detail::DetailView;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::{NavTarget, ObjectTarget, OpenedPanel};
use crate::ui::picker::PickerEvent;
use crate::util::shell::test_support::{
    crd_kind, focused, handles, press, temp_workspace_path, workspace,
};
use crate::util::shell::{MainWindow, WindowMode};
use gpui_kit::{TestAppContext, VisualTestContext, WindowHandle};
use kube::core::GroupVersionKind;

fn open(
    window: WindowHandle<MainWindow>,
    target: &NavTarget,
    cx: &mut VisualTestContext,
) -> Option<OpenedPanel> {
    window
        .update(cx, |main_window, _, _| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                return None;
            };
            open_panels
                .iter()
                .find(|open| &open.key.target == target)
                .and_then(|open| open.panel.clone())
        })
        .unwrap()
}

fn select_pod(cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })))
    });
}

/// Pods, brought up with Show Pods (cmd-1) from the Resource panel, takes `d`.
#[gpui_kit::test]
async fn pods_opened_from_the_keyboard_takes_its_keys(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    select_pod(&mut vcx);

    press(&mut vcx, "cmd-1");
    press(&mut vcx, "d");
    assert!(
        open(window, &NavTarget::pod("default", "web-1"), &mut vcx).is_some(),
        "Pods' `d` described the pod"
    );
}

/// A pod's detail, opened with `d`, takes `y`.
#[gpui_kit::test]
async fn pod_detail_opened_with_d_takes_its_keys(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    select_pod(&mut vcx);
    press(&mut vcx, "cmd-1");
    press(&mut vcx, "d");
    let Some(OpenedPanel::PodDetail(detail)) =
        open(window, &NavTarget::pod("default", "web-1"), &mut vcx)
    else {
        panic!("`d` opened the pod's detail")
    };

    press(&mut vcx, "y");
    assert_eq!(vcx.update(|_, cx| detail.read(cx).view()), DetailView::Yaml);
}

/// Services, as discovery reports them: a built-in kind, so its Resource panel row
/// isn't inside a collapsed Custom Resources group.
fn services() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// A kind's list, opened with Enter on its Resource panel row, takes `/`.
#[gpui_kit::test]
async fn an_object_list_opened_with_enter_takes_its_keys(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    window
        .update(&mut vcx, |main_window, _, cx| {
            main_window
                .test_resource_panel()
                .expect("a workspace")
                .update(cx, |panel, cx| panel.test_show_kinds(vec![services()], cx));
        })
        .unwrap();
    vcx.run_until_parked();

    press(&mut vcx, "down");
    press(&mut vcx, "enter");
    let Some(OpenedPanel::ObjectList(list)) = open(window, &NavTarget::Kind(services()), &mut vcx)
    else {
        panic!("Enter opened the kind's list")
    };
    press(&mut vcx, "/");
    assert!(
        vcx.update(|window, cx| list.read(cx).filter_focused(window, cx)),
        "the list's `/` focused its filter"
    );
}

/// One object, opened the way Enter on a list row opens it, takes `y`.
#[gpui_kit::test]
async fn an_object_detail_opened_from_a_list_takes_its_keys(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let target = ObjectTarget {
        kind: crd_kind(),
        namespace: Some("default".into()),
        name: "frond".into(),
    };
    // What the list's Enter dispatches (`ObjectListPanel::open_row`); a row
    // needs listed objects, which this window has none of.
    vcx.dispatch_action(OpenListedObject {
        context_name: "kind-dev".into(),
        target: target.clone(),
        view: None,
        mode: crate::ui::nav::OpenMode::Foreground,
    });
    vcx.run_until_parked();
    let Some(OpenedPanel::ObjectDetail(detail)) =
        open(window, &NavTarget::Object(target), &mut vcx)
    else {
        panic!("the object's detail opened")
    };

    press(&mut vcx, "y");
    assert_eq!(vcx.update(|_, cx| detail.read(cx).view()), DetailView::Yaml);
}

/// Logs, opened with Show Logs (cmd-2), holds focus. It has no keys of its own
/// to press yet, so focus is what's checked.
#[gpui_kit::test]
async fn logs_opened_from_the_keyboard_holds_focus(cx: &mut TestAppContext) {
    let window = workspace(cx);
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let resource = handles(window, &mut vcx).resource;

    press(&mut vcx, "cmd-2");
    let Some(OpenedPanel::Logs(logs)) = open(window, &NavTarget::Logs, &mut vcx) else {
        panic!("cmd-2 opened Logs")
    };
    let handle = vcx.update(|_, cx| gpui_kit::Focusable::focus_handle(logs.read(cx), cx));
    assert!(focused(window, &handle, &mut vcx));
    assert!(!focused(window, &resource, &mut vcx));
}

/// The app's setup, as `workspace` does it, with no window yet.
fn app(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        crate::util::shell::init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
}

/// The panel a window shows first - a new or restored workspace's Pods - has
/// focus as the window opens, so its keys work before any click.
#[gpui_kit::test]
async fn a_new_workspace_window_starts_on_its_displayed_panel(cx: &mut TestAppContext) {
    app(cx);
    let window = cx.add_window(|window, cx| {
        let main = MainWindow::test_workspace(vec!["kind-dev".into()], window, cx);
        // As `open_window` does at launch.
        main.focus_initial(window, cx);
        main
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    let pods = handles(window, &mut vcx).pods;
    assert!(focused(window, &pods, &mut vcx), "Pods has focus");
    select_pod(&mut vcx);
    press(&mut vcx, "d");
    assert!(open(window, &NavTarget::pod("default", "web-1"), &mut vcx).is_some());
}

/// Connecting from the picker swaps in a workspace whose displayed panel has focus.
#[gpui_kit::test]
async fn connecting_from_the_picker_focuses_the_displayed_panel(cx: &mut TestAppContext) {
    app(cx);
    let window = cx.add_window(|window, cx| {
        let main = MainWindow::test_picker_window(window, cx);
        if let WindowMode::Picker(picker) = &main.mode {
            crate::util::shell::window::watch_picker(picker, window, cx);
        }
        main.focus_initial(window, cx);
        main
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();
    window
        .update(&mut vcx, |main, _, cx| {
            let WindowMode::Picker(picker) = &main.mode else {
                panic!("a new window opens on the picker")
            };
            picker.update(cx, |_, cx| {
                cx.emit(PickerEvent::Connected {
                    context_name: "kind-dev".into(),
                })
            });
        })
        .unwrap();
    vcx.run_until_parked();

    let pods = handles(window, &mut vcx).pods;
    assert!(focused(window, &pods, &mut vcx), "Pods has focus");
}
