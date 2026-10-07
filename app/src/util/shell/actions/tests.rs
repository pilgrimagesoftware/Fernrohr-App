// Named imports rather than `use super::*`: a glob re-import of `gpui_kit::*`
// next to `#[gpui_kit::test]` items blows the macro-expansion budget (see
// `util/shell.rs`), and would shadow the built-in `#[test]`.
use crate::k8s::cluster::session::ClusterRegistry;
use crate::util::shell::test_support::*;
use crate::util::shell::{
    MainWindow, NavTarget, OpenPanel, OpenedPanel, ShowLogs, ShowPodDetail, WindowMode, init,
};
use gpui_kit::TestAppContext;

/// Section 5.1: the `ShowPodDetail` a Pods panel emits (for `d`, `y`, or
/// the row menu's "Open") resolves the pod from the app-scoped
/// `SelectedPod` and opens its detail panel - the same `open_target` the
/// keybinding-free path above uses.
#[gpui_kit::test]
async fn a_requested_pod_detail_opens_its_panel(cx: &mut TestAppContext) {
    use crate::k8s::resource::pods::{PodSelection, SelectedPod};

    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "prod".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })));
    });
    window
        .update(cx, |main_window, window, cx| {
            main_window.focus_handle.clone().focus(window, cx);
            window.dispatch_action(Box::new(ShowPodDetail), cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert!(
                open_panels
                    .iter()
                    .any(|open| open.key.target == NavTarget::pod("prod", "web-1")),
                "the selected pod's detail panel is open"
            );
        })
        .unwrap();
}

/// `y` is bound to "the YAML, now", so it has to land on the YAML - and it
/// has to do so whether the panel is opened by the shortcut or already
/// sitting there showing fields, which is the case that actually comes up.
///
/// Both halves matter and they fail differently: a fresh open needs the
/// view threaded into construction, and an existing panel needs the window
/// to hold on to the entity so it can switch a panel it only has a
/// `PanelId` for.
#[gpui_kit::test]
async fn asking_for_yaml_opens_and_switches_the_pod_panel_to_yaml(cx: &mut TestAppContext) {
    use crate::k8s::resource::pod_detail::DetailView;
    use crate::k8s::resource::pods::{PodSelection, SelectedPod};
    use crate::ui::nav::ShowPodDetailYaml;

    type Window = gpui_kit::WindowHandle<MainWindow>;

    /// Dispatches an app-level detail request the way a keybinding would.
    fn request_detail(cx: &mut TestAppContext, window: &Window, yaml: bool) {
        window
            .update(cx, |main_window, window, cx| {
                main_window.focus_handle.clone().focus(window, cx);
                if yaml {
                    window.dispatch_action(Box::new(ShowPodDetailYaml), cx);
                } else {
                    window.dispatch_action(Box::new(ShowPodDetail), cx);
                }
            })
            .unwrap();
        cx.run_until_parked();
    }

    /// The view showing in the window's one pod detail panel.
    fn open_panel_view(cx: &mut TestAppContext, window: &Window) -> DetailView {
        window
            .update(cx, |main_window, _window, cx| {
                let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                let detail: Vec<&OpenPanel> = open_panels
                    .iter()
                    .filter(|open| matches!(open.panel, Some(OpenedPanel::PodDetail(_))))
                    .collect();
                assert_eq!(
                    detail.len(),
                    1,
                    "one detail panel, whichever shortcut asked for it"
                );
                let Some(OpenedPanel::PodDetail(panel)) = &detail[0].panel else {
                    unreachable!("filtered to detail panels")
                };
                panel.read(cx).view()
            })
            .unwrap()
    }

    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "prod".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })));
    });

    request_detail(cx, &window, true);
    assert_eq!(
        open_panel_view(cx, &window),
        DetailView::Yaml,
        "a fresh panel opens on the YAML, not on its default view"
    );

    request_detail(cx, &window, true);
    assert_eq!(
        open_panel_view(cx, &window),
        DetailView::Yaml,
        "asking again is still one panel, and still on the YAML"
    );

    request_detail(cx, &window, false);
    assert_eq!(
        open_panel_view(cx, &window),
        DetailView::Structured,
        "`d` switches the open panel back to the field list rather than \
         adding a second panel for the same pod"
    );
}

/// `1-window-context-bar` bug 1: a pod selected from one context's Pods panel
/// must open Logs/Pod-detail against *that* context, not whichever context this
/// window's Resource panel dropdown currently shows. This was the bug's exact
/// shape - `open_target_with_view` built the new panel's scope from
/// `contexts[active]` regardless of which context the selected pod actually
/// came from, turning a real pod into `pods "..." not found`.
///
/// Two pre-seeded sessions (never a real connect - see `pod_scoped_context`'s
/// doc comment and `MainWindow::test_workspace`'s), the second context's pod
/// selected while the first stays active.
#[gpui_kit::test]
async fn show_logs_and_pod_detail_use_the_selected_pods_context_not_the_active_one(
    cx: &mut TestAppContext,
) {
    use crate::k8s::cluster::connection::ConnectionState;
    use crate::k8s::resource::pods::{PodSelection, SelectedPod};

    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "northbay", ConnectionState::Connecting);
        ClusterRegistry::insert_test_session(cx, "other-context", ConnectionState::Connecting);
    });

    let window = cx.add_window(|window, cx| {
        MainWindow::test_workspace(
            vec!["northbay".to_string(), "other-context".to_string()],
            window,
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        window
            .update(cx, |main_window, _window, _cx| {
                main_window.test_active_context_name()
            })
            .unwrap(),
        Some("northbay".to_string()),
        "the window opens on its first context"
    );

    // The pod was selected from the *other* context's Pods panel - active stays
    // `northbay` throughout, matching the bug report exactly.
    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "clamav-plc9g".into(),
            containers: vec!["clamav".into()],
            context_name: "other-context".into(),
        })));
    });

    window
        .update(cx, |main_window, window, cx| {
            main_window.focus_handle.clone().focus(window, cx);
            window.dispatch_action(Box::new(ShowLogs), cx);
            window.dispatch_action(Box::new(ShowPodDetail), cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let logs = open_panels
                .iter()
                .find(|open| matches!(open.key.target, NavTarget::Logs | NavTarget::PodLogs(_)))
                .expect("ShowLogs opened a panel");
            assert_eq!(
                logs.key.context_name, "other-context",
                "Logs must stream from the context that published the selected pod, \
                 not the window's active one"
            );

            let detail = open_panels
                .iter()
                .find(|open| open.key.target == NavTarget::pod("default", "clamav-plc9g"))
                .expect("ShowPodDetail opened a panel");
            assert_eq!(
                detail.key.context_name, "other-context",
                "the pod's detail panel must read from the context that selected it"
            );
        })
        .unwrap();
}

/// The other half of `pod_scoped_context`'s contract: a selection from a
/// context this window does not hold at all (a stale global, or a pod picked in
/// a window that has since disconnected that context) must not silently open
/// against the active context instead - it is refused and logged.
#[gpui_kit::test]
async fn show_logs_no_ops_when_the_selected_pods_context_is_not_open_here(cx: &mut TestAppContext) {
    use crate::k8s::cluster::connection::ConnectionState;
    use crate::k8s::resource::pods::{PodSelection, SelectedPod};

    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    let window =
        cx.add_window(|window, cx| MainWindow::test_workspace(vec!["kind-dev".into()], window, cx));
    cx.run_until_parked();

    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "never-added".into(),
        })));
    });

    window
        .update(cx, |main_window, window, cx| {
            main_window.focus_handle.clone().focus(window, cx);
            window.dispatch_action(Box::new(ShowLogs), cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert!(
                !open_panels
                    .iter()
                    .any(|open| matches!(open.key.target, NavTarget::Logs | NavTarget::PodLogs(_))),
                "a selection from a context this window doesn't hold must not open Logs \
                 against the active context instead"
            );
        })
        .unwrap();
}

/// Keyboard entry into the Resource panel: the Focus Resources key moves focus onto
/// its list with no click first, so the panel's own keys work straight away.
#[gpui_kit::test]
async fn the_focus_resources_key_focuses_the_resource_panel(cx: &mut TestAppContext) {
    use crate::k8s::cluster::connection::ConnectionState;
    use gpui_kit::VisualTestContext;

    cx.executor().allow_parking();
    let path = temp_workspace_path();
    let keymap_path = temp_workspace_path();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        init(cx, path.clone(), &keymap_path);
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    let window = cx.add_window(|window, cx| {
        MainWindow::test_workspace(vec!["kind-dev".to_string()], window, cx)
    });
    // Something has focus in a real window from launch (`focus_initial`); a key only
    // dispatches along the focused element's path, so give this one the same start.
    window
        .update(cx, |main_window, window, cx| {
            window.focus(&main_window.focus_handle, cx);
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    vcx.run_until_parked();

    let key = gpui_kit::Keystroke::parse("cmd-0")
        .expect("valid")
        .unparse();
    vcx.simulate_keystrokes(&key);
    vcx.run_until_parked();

    let focused = window
        .update(&mut vcx, |main_window, window, cx| {
            main_window
                .test_resource_panel()
                .expect("a workspace window has a Resource panel")
                .read(cx)
                .is_list_focused(window)
        })
        .unwrap();
    assert!(
        focused,
        "the Focus Resources key puts focus on the Resource panel"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&keymap_path);
}
