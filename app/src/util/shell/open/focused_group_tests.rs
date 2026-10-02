//! Where a newly opened panel lands (`0-open-in-focused-group`): in the tab group
//! that last had focus, even when the request comes from outside the dock - the
//! Resource panel's Enter here. Two stacked groups, in a `Root` window with the
//! app's own keymap: Pods above, a pod's detail split below it.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::pods::{PodSelection, SelectedPod};
use crate::ui::nav::{NavTarget, OpenedPanel};
use crate::util::shell::test_support::{press, temp_workspace_path};
use crate::util::shell::{MainWindow, WindowMode, init};
use gpui_kit::component::Root;
use gpui_kit::component::dock::{InsertTarget, NodeId, PanelId};
use gpui_kit::{AppContext as _, Entity, FocusHandle, TestAppContext, VisualTestContext};
use kube::core::GroupVersionKind;

fn services() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Service"),
        plural: "services".into(),
        namespaced: true,
    }
}

struct Harness {
    vcx: VisualTestContext,
    main: Entity<MainWindow>,
    pods: PanelId,
    lower: PanelId,
    lower_focus: FocusHandle,
}

/// Pods in the upper group, `web-1`'s detail alone in a group split below it,
/// and Pods then Services listed in the Resource panel. Focus is on the Pods
/// panel; the lower group hasn't had focus.
fn harness(cx: &mut TestAppContext) -> Harness {
    cx.executor().allow_parking();
    let (workspace, keymap) = (temp_workspace_path(), temp_workspace_path());
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace, &keymap);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })));
    });
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let main = cx.new(|cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
        // As `open_window` does at launch.
        main.update(cx, |main, cx| main.focus_initial(window, cx));
        built = Some(main.clone());
        Root::new(main, window, cx)
    });
    let main = built.expect("the window built its view");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    // The key window, as the OS makes it: GPUI reports focus changes - which the
    // window's last-focused tracking listens to - only for the active window.
    vcx.update(|window, _| window.activate_window());
    vcx.update(|_, cx| {
        main.read(cx)
            .test_resource_panel()
            .unwrap()
            .update(cx, |panel, cx| {
                panel.test_show_kinds(vec![DiscoveredKind::pods(), services()], cx)
            })
    });
    vcx.run_until_parked();
    press(&mut vcx, "cmd-1");
    press(&mut vcx, "d");
    let (pods, lower, lower_focus) = vcx.update(|window, cx| {
        let (pods, lower, lower_focus) = {
            let WindowMode::Workspace { open_panels, .. } = &main.read(cx).mode else {
                panic!("a connected window is in workspace mode")
            };
            let find = |target: &NavTarget| {
                open_panels
                    .iter()
                    .find(|open| &open.key.target == target)
                    .expect("open")
            };
            let detail = find(&NavTarget::pod("default", "web-1"));
            let Some(OpenedPanel::PodDetail(panel)) = &detail.panel else {
                panic!("a pod detail")
            };
            (
                find(&NavTarget::pods()).id,
                detail.id,
                gpui_kit::Focusable::focus_handle(panel.read(cx), cx),
            )
        };
        let WindowMode::Workspace { dock_area, .. } = &main.read(cx).mode else {
            unreachable!()
        };
        dock_area.clone().update(cx, |area, cx| {
            let node = group(area, pods);
            let split = InsertTarget::Split {
                node,
                placement: gpui_kit::base::Placement::Bottom,
                size: None,
            };
            area.move_panel(lower, split, window, cx);
        });
        (pods, lower, lower_focus)
    });
    vcx.run_until_parked();
    Harness {
        vcx,
        main,
        pods,
        lower,
        lower_focus,
    }
}

fn group(area: &gpui_kit::component::dock::DockArea, panel: PanelId) -> NodeId {
    crate::ui::panel::tabs::group_of(area, panel).expect("docked")
}

fn group_of(h: &mut Harness, panel: PanelId) -> NodeId {
    h.vcx.update(|_, cx| {
        let WindowMode::Workspace { dock_area, .. } = &h.main.read(cx).mode else {
            panic!("a connected window is in workspace mode")
        };
        group(dock_area.read(cx), panel)
    })
}

/// Opens Services from the Resource panel by keyboard: Focus Resources, down to
/// its row, Enter. Returns the new panel.
fn open_services_from_the_resource_panel(h: &mut Harness) -> PanelId {
    press(&mut h.vcx, "cmd-0");
    press(&mut h.vcx, "down");
    press(&mut h.vcx, "down");
    press(&mut h.vcx, "enter");
    h.vcx.update(|_, cx| {
        let WindowMode::Workspace { open_panels, .. } = &h.main.read(cx).mode else {
            panic!("a connected window is in workspace mode")
        };
        open_panels
            .iter()
            .find(|open| open.key.target == NavTarget::Kind(services()))
            .expect("Enter opened Services")
            .id
    })
}

/// The report: focus the lower group, then open from the Resource panel - the
/// new panel joins the lower group, not the upper one.
#[gpui_kit::test]
async fn opening_from_the_resource_panel_joins_the_last_focused_group(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let (pods, lower) = (h.pods, h.lower);
    assert_ne!(
        group_of(&mut h, pods),
        group_of(&mut h, lower),
        "two groups"
    );
    let lower_focus = h.lower_focus.clone();
    h.vcx.update(|window, cx| window.focus(&lower_focus, cx));
    h.vcx.run_until_parked();

    let opened = open_services_from_the_resource_panel(&mut h);
    assert_eq!(
        group_of(&mut h, opened),
        group_of(&mut h, lower),
        "Services opened in the lower group"
    );
}

/// The upper group last had focus: that's where the panel goes.
#[gpui_kit::test]
async fn the_upper_group_when_it_had_focus_last(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let lower_focus = h.lower_focus.clone();
    h.vcx.update(|window, cx| window.focus(&lower_focus, cx));
    h.vcx.run_until_parked();
    press(&mut h.vcx, "cmd-1");

    let opened = open_services_from_the_resource_panel(&mut h);
    let pods = h.pods;
    assert_eq!(group_of(&mut h, opened), group_of(&mut h, pods));
}

/// Re-showing a panel that's already open focuses it where it is: Services opened
/// below stays below when picked again with the upper group focused.
#[gpui_kit::test]
async fn reopening_an_open_panel_leaves_it_where_it_is(cx: &mut TestAppContext) {
    let mut h = harness(cx);
    let lower_focus = h.lower_focus.clone();
    h.vcx.update(|window, cx| window.focus(&lower_focus, cx));
    h.vcx.run_until_parked();
    let opened = open_services_from_the_resource_panel(&mut h);
    press(&mut h.vcx, "cmd-1");

    let again = open_services_from_the_resource_panel(&mut h);
    assert_eq!(again, opened, "the same panel");
    let lower = h.lower;
    assert_eq!(group_of(&mut h, again), group_of(&mut h, lower));
}
