// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::*;
use crate::util::shell::{NavTarget, PanelKey, WindowMode};
use gpui_kit::TestAppContext;
use gpui_kit::component::dock::{DockLayout, DockPlacement};

/// Section 9.1: selecting a kind adds a panel to the dock rather than
/// replacing what was there, and it reuses the window's connection instead
/// of opening a new one - so the kinds listed and the panels opened stay on
/// one `ClusterSession`.
#[gpui_kit::test]
async fn opening_a_kind_adds_a_panel_and_keeps_the_session(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    let session_before = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());

    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let before = dock_area
                .read(cx)
                .layout(DockPlacement::Center)
                .expect("a workspace dock has a centre")
                .panels()
                .count();

            main_window.open_target(NavTarget::Kind(crd_kind()), window, cx);

            let dock_area = match &main_window.mode {
                WindowMode::Workspace { dock_area, .. } => dock_area,
                _ => unreachable!("open_target did not leave workspace mode"),
            };
            let after = dock_area
                .read(cx)
                .layout(DockPlacement::Center)
                .expect("a workspace dock has a centre")
                .panels()
                .count();
            assert_eq!(after, before + 1, "the kind got its own panel");
        })
        .unwrap();
    cx.run_until_parked();

    let session_after = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());
    assert_eq!(
        session_before, session_after,
        "opening a panel must not reconnect the window"
    );
}

/// Section 9.3: a kind that already has a panel open is focused rather than
/// opened a second time. The workspace starts with Pods open, so the first
/// selection of Pods is already the "already open" case.
#[gpui_kit::test]
async fn reopening_a_kind_focuses_it_instead_of_duplicating(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(
                open_panels.len(),
                1,
                "the workspace opens Pods and records it"
            );
            assert_eq!(
                open_panels[0].key,
                PanelKey {
                    target: NavTarget::pods(),
                    context_name: "kind-dev".to_string(),
                    namespaces: Vec::new(),
                }
            );

            // A second, different kind is a genuinely new panel...
            main_window.open_target(NavTarget::Kind(crd_kind()), window, cx);
            // ...and the first one again, which must not add a third.
            main_window.open_target(NavTarget::pods(), window, cx);
            main_window.open_target(NavTarget::pods(), window, cx);

            let WindowMode::Workspace {
                open_panels,
                dock_area,
                ..
            } = &main_window.mode
            else {
                unreachable!("open_target did not leave workspace mode")
            };
            assert_eq!(
                open_panels.len(),
                2,
                "two distinct kinds, however many times each is selected"
            );
            let distinct: std::collections::HashSet<_> =
                open_panels.iter().map(|open| open.key.clone()).collect();
            assert_eq!(distinct.len(), 2, "the recorded keys are distinct");
            assert_eq!(
                dock_area
                    .read(cx)
                    .layout(DockPlacement::Center)
                    .expect("a workspace dock has a centre")
                    .panels()
                    .count(),
                2,
                "the dock holds one panel per distinct kind"
            );
        })
        .unwrap();
    cx.run_until_parked();
}

/// Closing a panel frees its key, so re-selecting the kind afterwards opens
/// a fresh panel instead of focusing a dock id the area no longer holds.
/// The dock has no id-keyed removal, so the centre is emptied the way the
/// app's own "last panel closed" path empties it.
#[gpui_kit::test]
async fn a_closed_kind_is_opened_again_rather_than_focused(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            let dock_area = match &main_window.mode {
                WindowMode::Workspace { dock_area, .. } => dock_area.clone(),
                _ => panic!("a connected window is in workspace mode"),
            };
            dock_area.update(cx, |area, cx| {
                area.set_center(DockLayout::tabs(), window, cx);
            });
            main_window.forget_closed_panels(&dock_area, cx);

            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                unreachable!()
            };
            assert!(open_panels.is_empty(), "the closed panel was forgotten");

            main_window.open_target(NavTarget::pods(), window, cx);
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                unreachable!()
            };
            assert_eq!(open_panels.len(), 1, "so the kind opens again");
        })
        .unwrap();
    cx.run_until_parked();
}

/// Splits the centre into an upper group (holding the workspace's Pods list) and
/// a lower group (holding a second panel), and returns the lower panel's id.
fn split_with_a_lower_panel(
    main_window: &mut crate::util::shell::MainWindow,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<crate::util::shell::MainWindow>,
) -> gpui_kit::component::dock::PanelId {
    use gpui_kit::component::dock::InsertTarget;

    main_window.open_target(NavTarget::Kind(crd_kind()), window, cx);
    let WindowMode::Workspace {
        dock_area,
        open_panels,
        ..
    } = &main_window.mode
    else {
        panic!("a connected window is in workspace mode")
    };
    let lower = open_panels
        .iter()
        .find(|open| open.key.target == NavTarget::Kind(crd_kind()))
        .expect("the kind's panel opened")
        .id;
    dock_area.update(cx, |area, cx| {
        let tree = area.layout(DockPlacement::Center).expect("a centre");
        let group = tree.find_panel_node(lower).expect("docked");
        let split = InsertTarget::Split {
            node: group,
            placement: gpui_kit::base::Placement::Bottom,
            size: None,
        };
        area.move_panel(lower, split, window, cx);
    });
    lower
}

/// The tab group `id` is in.
fn group_of(
    main_window: &crate::util::shell::MainWindow,
    id: gpui_kit::component::dock::PanelId,
    cx: &gpui_kit::App,
) -> gpui_kit::component::dock::NodeId {
    let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
        panic!("a connected window is in workspace mode")
    };
    dock_area
        .read(cx)
        .layout(DockPlacement::Center)
        .expect("a centre")
        .find_panel_node(id)
        .expect("docked")
}

fn opened_id(
    main_window: &crate::util::shell::MainWindow,
    target: &NavTarget,
) -> gpui_kit::component::dock::PanelId {
    let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
        panic!("a connected window is in workspace mode")
    };
    open_panels
        .iter()
        .find(|open| &open.key.target == target)
        .expect("the target's panel opened")
        .id
}

/// A panel opened while another has focus - the double-click or `g` it came
/// from - joins that panel's tab group: from the lower half of a split, the new
/// panel opens in the lower half, not the dock's first group.
#[gpui_kit::test]
async fn a_panel_opened_from_a_lower_split_opens_beside_it(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            let lower = split_with_a_lower_panel(main_window, window, cx);
            let lower_group = group_of(main_window, lower, cx);
            let pods = opened_id(main_window, &NavTarget::pods());
            assert_ne!(
                group_of(main_window, pods, cx),
                lower_group,
                "the split put the two panels in different groups"
            );

            // Focus inside the lower panel, as a double-click there leaves it.
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                unreachable!()
            };
            let handle = dock_area
                .read(cx)
                .panel(lower)
                .expect("docked")
                .focus_handle(cx);
            window.focus(&handle, cx);

            let pod = NavTarget::pod("staging", "web-1");
            main_window.open_target(pod.clone(), window, cx);
            let opened = opened_id(main_window, &pod);
            assert_eq!(
                group_of(main_window, opened, cx),
                lower_group,
                "the pod opened in the group it was opened from"
            );
        })
        .unwrap();
}

/// With focus outside the dock (the Resource panel, or nowhere), a new panel
/// goes where it always did: the dock's first group.
#[gpui_kit::test]
async fn a_panel_opened_from_outside_the_dock_uses_the_first_group(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            let lower = split_with_a_lower_panel(main_window, window, cx);
            let pods = opened_id(main_window, &NavTarget::pods());
            let upper_group = group_of(main_window, pods, cx);
            main_window.focus_handle.clone().focus(window, cx);

            let pod = NavTarget::pod("staging", "web-1");
            main_window.open_target(pod.clone(), window, cx);
            let opened = opened_id(main_window, &pod);
            assert_eq!(group_of(main_window, opened, cx), upper_group);
            assert_ne!(
                group_of(main_window, opened, cx),
                group_of(main_window, lower, cx)
            );
        })
        .unwrap();
}
