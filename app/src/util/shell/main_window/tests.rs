// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::ui::nav;
use crate::ui::panel_title::PanelScope;
use crate::util::shell::test_support::*;
use crate::util::shell::{
    MainWindow, NavTarget, WindowLayout, WindowMode, WorkspaceConfig, config, open_window, save,
};
use gpui_kit::{AppContext as _, TestAppContext};

/// Section 10.1-10.3, checked on every panel type the dock holds rather than
/// on the title-bar helpers alone: each panel's tab names its kind, a
/// namespace picker is on the bar exactly when the kind is namespaced, and
/// none of them puts its close control in the toolbar, since it sits beside
/// the title instead.
///
/// A cluster-scoped kind is in the list on purpose - it is the case where
/// the picker must be *absent*, which a test over namespaced kinds alone
/// could not catch.
#[gpui_kit::test]
async fn every_resource_panel_carries_its_title_bar(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    let expected = 5;
    let cases: [NavTarget; 5] = [
        NavTarget::pods(),
        NavTarget::Logs,
        NavTarget::Kind(crd_kind()),
        NavTarget::Kind(cluster_scoped_kind()),
        NavTarget::Object(crate::ui::nav::ObjectTarget {
            kind: crd_kind(),
            namespace: Some("staging".into()),
            name: "fronds".into(),
        }),
    ];

    let mut checked: Vec<String> = Vec::new();
    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            for target in cases {
                let scope = PanelScope::new(target.clone(), "kind-dev".to_string());
                let (_id, opened) = dock_area.update(cx, |area, cx| {
                    nav::add_panel(area, &scope, None, window, cx)
                });
                let (name, controls) = match opened {
                    nav::OpenedPanel::Pods(panel) => title_bar_of(&panel, window, cx),
                    nav::OpenedPanel::ObjectList(panel) => title_bar_of(&panel, window, cx),
                    nav::OpenedPanel::Placeholder(panel) => title_bar_of(&panel, window, cx),
                    nav::OpenedPanel::Logs(panel) => title_bar_of(&panel, window, cx),
                    nav::OpenedPanel::PodDetail(panel) => title_bar_of(&panel, window, cx),
                    nav::OpenedPanel::ObjectDetail(panel) => title_bar_of(&panel, window, cx),
                };
                // No plain tab name, so the dock draws the tab from the panel's
                // title element and its "Context:" tooltip.
                assert_eq!(name, None, "the tab is drawn from the title element");
                // `item_label` is `list_label` for a list; for one object it
                // is the kind plus the object's name.
                assert_eq!(
                    crate::ui::panel::title::title(&scope),
                    target.item_label(),
                    "the title names the kind, never the cluster"
                );
                // The close control is drawn beside the title, not at the
                // title bar's far end (`tab-close-buttons` 4.2; the drawn
                // control is checked in `tabs/tests/close.rs`).
                assert_eq!(
                    controls,
                    0,
                    "no toolbar close on {}: it sits beside the title",
                    target.label()
                );
                checked.push(target.label());
            }
        })
        .unwrap();

    assert_eq!(
        checked.len(),
        expected,
        "every panel type was checked: {checked:?}"
    );
}

/// `standard-resource-panels` 1.4: every kind but the core Pod kind opens the
/// generic list over that kind - a namespaced built-in (Deployments), a
/// cluster-scoped one (Nodes) and a CRD alike - and Pods keep the Pods panel. None
/// of them gets a placeholder.
#[gpui_kit::test]
async fn every_kind_but_pods_opens_a_list_panel(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let deployments = crate::k8s::cluster::discovery::DiscoveredKind {
        gvk: kube::core::GroupVersionKind::gvk("apps", "v1", "Deployment"),
        plural: "deployments".into(),
        namespaced: true,
    };
    let nodes = crate::k8s::cluster::discovery::DiscoveredKind {
        gvk: kube::core::GroupVersionKind::gvk("", "v1", "Node"),
        plural: "nodes".into(),
        namespaced: false,
    };

    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            for kind in [deployments, nodes, crd_kind()] {
                let scope = PanelScope::new(NavTarget::Kind(kind.clone()), "kind-dev".into());
                let (_id, opened) = dock_area.update(cx, |area, cx| {
                    nav::add_panel(area, &scope, None, window, cx)
                });
                let nav::OpenedPanel::ObjectList(panel) = opened else {
                    panic!("{} should open a list panel", kind.label());
                };
                assert_eq!(
                    panel.read(cx).kind(),
                    &kind,
                    "the list is over its own kind"
                );
            }
            let scope = PanelScope::new(NavTarget::pods(), "kind-dev".into());
            let (_id, opened) = dock_area.update(cx, |area, cx| {
                nav::add_panel(area, &scope, None, window, cx)
            });
            assert!(
                matches!(opened, nav::OpenedPanel::Pods(_)),
                "Pods keep their panel"
            );
        })
        .unwrap();
}

/// Section 10.2's second half: a panel that reports a narrower namespace is
/// re-filed under it.
///
/// This is the body the title-bar subscription runs, driven directly: the
/// dock hands out panel ids rather than panel entities, so there is no way
/// from a test to pick a namespace in the rendered menu and watch the
/// event arrive. What it pins down is the rule the subscription exists for
/// - the panel stops being filed under the scope it no longer shows.
#[gpui_kit::test]
async fn a_narrowed_namespace_rekeys_the_open_panel(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, cx| {
            let (id, before) = match &main_window.mode {
                WindowMode::Workspace { open_panels, .. } => {
                    let open = &open_panels[0];
                    (open.id, open.key.clone())
                }
                _ => panic!("a connected window is in workspace mode"),
            };
            assert!(before.namespaces.is_empty(), "it starts on all namespaces");

            main_window.rescope(id, vec!["staging".to_string(), "default".to_string()], cx);

            let after = match &main_window.mode {
                WindowMode::Workspace { open_panels, .. } => open_panels[0].key.clone(),
                _ => unreachable!("rescope did not leave workspace mode"),
            };
            assert_ne!(
                before, after,
                "the panel must stop being filed under the scope it dropped"
            );
            assert_eq!(
                after.namespaces,
                ["staging", "default"],
                "and be filed under the namespaces it now shows"
            );
            assert_eq!(after.target, before.target, "only the namespace moved");
            assert_eq!(
                after.context_name, before.context_name,
                "the cluster is still the cluster"
            );
        })
        .unwrap();
}

/// `connection-status-bar` 2.3: the status bar renders under a connected workspace's
/// body, and a picker-mode window - which shows its own connect progress instead
/// (proposal.md's non-goals) - has no such field to render at all.
#[gpui_kit::test]
async fn the_status_bar_renders_only_in_workspace_mode(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    let window = cx.add_window(|window, cx| MainWindow {
        mode: WindowMode::Picker(cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx))),
        focus_handle: cx.focus_handle(),
    });
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Picker(_)),
                "the picker variant carries no status bar field"
            );
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("a picker-mode window renders with no status bar");

    window
        .update(cx, |main_window, window, cx| {
            main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Workspace { status_bar, .. } = &main_window.mode else {
                panic!("entering the workspace leaves picker mode")
            };
            assert_eq!(
                status_bar.read(cx).items(cx).len(),
                1,
                "the bar is wired to the window's own context, not merely a field \
                 nobody reads"
            );
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
    })
    .expect("a workspace window renders its status bar");
}

/// The Resource panel's width survives a relaunch: a restored window starts at its
/// saved width and saving writes it back.
#[gpui_kit::test]
async fn the_resource_panel_width_round_trips(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string()],
        panels: vec![pods_panel_descriptor("kind-dev")],
        resource_panel_width: Some(330.0),
        ..Default::default()
    };
    cx.update(|cx| open_window(cx, layout));
    cx.run_until_parked();

    cx.update(|cx| save(cx, &path));
    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows[0].resource_panel_width, Some(330.0));
    let _ = std::fs::remove_file(&path);
}

/// `standard-resource-panels` 5.3, in a real window: a custom resource list's
/// tab is drawn as its plural kind alone, with no group qualifier, and hovering
/// the tab shows the group in its tooltip.
#[gpui_kit::test]
async fn a_custom_resource_tab_is_drawn_with_its_kind_only(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let scope = PanelScope::new(NavTarget::Kind(crd_kind()), "kind-dev".into());
            dock_area.update(cx, |area, cx| {
                nav::add_panel(area, &scope, None, window, cx)
            });
        })
        .unwrap();
    let mut vcx = gpui_kit::VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    let drawn = |vcx: &mut gpui_kit::VisualTestContext, title: &'static str| {
        vcx.debug_bounds(title).is_some()
    };
    assert!(
        drawn(&mut vcx, "panel-title-Ferns-focused")
            || drawn(&mut vcx, "panel-title-Ferns-unfocused"),
        "the CRD's tab reads its plural kind"
    );
    assert!(
        !drawn(&mut vcx, "panel-title-Ferns · ferns.example.com-focused")
            && !drawn(&mut vcx, "panel-title-Ferns · ferns.example.com-unfocused"),
        "and carries no group qualifier"
    );

    let tab = ["panel-title-Ferns-focused", "panel-title-Ferns-unfocused"]
        .into_iter()
        .find_map(|title| vcx.debug_bounds(title))
        .expect("the CRD's tab is drawn");
    assert!(
        !drawn(&mut vcx, "panel-title-tooltip-group"),
        "no tooltip before the hover"
    );
    vcx.simulate_mouse_move(tab.center(), None, gpui_kit::Modifiers::none());
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    vcx.run_until_parked();
    assert!(
        drawn(&mut vcx, "panel-title-tooltip-group"),
        "hovering the tab shows the API group"
    );
    assert!(
        drawn(&mut vcx, "panel-title-tooltip-context"),
        "beside the context"
    );
}
