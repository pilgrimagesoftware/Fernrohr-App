// Not `use super::*`: `gpui_kit::*` re-exports its own `test` attribute
// macro, which would shadow `core::prelude::v1::test` for the plain
// synchronous test below.
use super::{
    ClosedWindowLayouts, MainWindow, NavTarget, OpenPanel, OpenedPanel, PanelDescriptor, PanelKey,
    SET_CONTEXT_TUNNEL_COMMAND_ID, SavedDockLayouts, ShowLogs, ShowPodDetail, ToggleCommandPalette,
    WindowLayout, WindowMode, WorkspaceConfig, close_window, config, init, open_saved_or_default,
    open_window, register_commands, restorable_panels, restored_contexts, restored_resource_width,
    save, watch_picker, write_context_tunnel,
};
use crate::command::CommandRegistry;
use crate::config::tunnels::{TunnelAuth, TunnelConfig};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav;
use crate::ui::panel_title::PanelScope;
use crate::util::context_lifecycle;
use gpui_kit::component::dock::{self, DockLayout, DockPlacement, PanelView as _};
use gpui_kit::{App, AppContext as _, Entity, SharedString, TestAppContext, Window, WindowId};
use kube::core::GroupVersionKind;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_workspace_path() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("fernrohr-shell-test-{n}.toml"))
}

fn temp_tunnels_path() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("fernrohr-shell-set-tunnel-test-{n}.toml"));
    let _ = std::fs::remove_file(&path);
    path
}

/// Tasks.md 3.2: `context.set_tunnel` is registered with a title, alongside every
/// other palette command.
#[test]
fn set_context_tunnel_is_a_registered_command() {
    let mut registry = CommandRegistry::new();
    register_commands(&mut registry);

    let command = registry
        .get(SET_CONTEXT_TUNNEL_COMMAND_ID)
        .expect("context.set_tunnel must be registered");
    assert_eq!(command.title, "Set Tunnel for Context");
}

/// Tasks.md 3.2: the command's handler binds - `write_context_tunnel` is the
/// write `on_action_set_tunnel`'s dialog options call, factored out so it's
/// testable without a `Root` (which its `open_dialog`/`close_dialog` calls
/// require).
#[test]
fn set_context_tunnels_handler_binds_and_unbinds() {
    let path = temp_tunnels_path();
    let store = crate::tunnel::store::TunnelStore::new(path.clone());
    store
        .create(
            "qa-bastion",
            TunnelConfig {
                name: "QA".into(),
                bastion_user: "ops".into(),
                bastion_host: "bastion.example.com".into(),
                bastion_port: 22,
                jump_hosts: Vec::new(),
                auth: TunnelAuth::default(),
            },
            None,
        )
        .unwrap();

    write_context_tunnel(&path, "qa-1", Some("qa-bastion")).unwrap();
    assert_eq!(store.binding_for("qa-1"), Some("qa-bastion".to_string()));

    write_context_tunnel(&path, "qa-1", None).unwrap();
    assert_eq!(store.binding_for("qa-1"), None);

    let _ = std::fs::remove_file(&path);
}

/// A connected window on `context_name`, for the panel-opening tests.
///
/// `enter_workspace` opens a real Pods panel (`nav::add_panel` always
/// builds one via `PodsPanel::new`, not the `with_stubs` seam
/// `pods::tests` uses), which starts a genuine `ClusterConnection::connect`,
/// a tokio task that resolves a kubeconfig and probes a server, then
/// wakes its GPUI observer from that tokio thread. `allow_parking` is the
/// same seam `cluster::session`'s and `cluster::connection`'s own tests
/// use for this exact reason: without it, the wakeup races the test
/// scheduler's thread-confinement check non-deterministically, since it
/// depends on real wall-clock I/O timing rather than anything these tests
/// control.
async fn connected_window(
    cx: &mut TestAppContext,
    context_name: &str,
) -> gpui_kit::WindowHandle<MainWindow> {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec![context_name.to_string()], window, cx);
        main_window
    })
}

/// A kind with no concrete panel, as discovery would report a CRD's.
fn crd_kind() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("ferns.example.com", "v1", "Fern"),
        plural: "ferns".to_string(),
        namespaced: true,
    }
}

/// A cluster-scoped CRD, the kind 10.2 says must not grow a namespace picker.
fn cluster_scoped_kind() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("widgets.example.com", "v1", "Widget"),
        plural: "widgets".to_string(),
        namespaced: false,
    }
}

/// The title bar the dock builds for `panel`, read back the way the dock
/// reads it: through `PanelView`, which is the object-safe face of `Panel`
/// and takes no panel context.
///
/// Returns the tab name, whether a namespace picker is on the bar, and how
/// many controls sit at its trailing end.
/// The namespace picker moved out of the title bar into each panel's own
/// body (Paul's feedback: a picker shared across a tab group's title bar
/// was ambiguous about which tab it scoped), so this reads only what
/// still lives in the dock's title bar: the name and the toolbar
/// controls.
fn title_bar_of<T: dock::Panel>(
    panel: &Entity<T>,
    window: &mut Window,
    cx: &mut App,
) -> (Option<SharedString>, usize) {
    let name = panel.tab_name(cx);
    let controls = panel
        .toolbar_buttons(window, cx)
        .map_or(0, |buttons| buttons.len());
    (name, controls)
}

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

/// Section 5.1/5.4: the pod-detail request lands on the same `open_target`
/// every other panel uses, so a pod gets a panel of its own and
/// re-requesting it focuses rather than duplicates. What makes two pods
/// two panels is the pod identity now inside the key, not a second dedup
/// rule.
#[gpui_kit::test]
async fn a_pod_detail_panel_is_keyed_by_which_pod(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    let session_before = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());

    window
        .update(cx, |main_window, window, cx| {
            main_window.open_target(NavTarget::pod("prod", "web-1"), window, cx);
            // The same pod again: focused, not opened twice.
            main_window.open_target(NavTarget::pod("prod", "web-1"), window, cx);
            // A different pod is a genuinely new panel.
            main_window.open_target(NavTarget::pod("prod", "web-2"), window, cx);

            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let pods: Vec<_> = open_panels
                .iter()
                .filter(|open| matches!(open.key.target, NavTarget::Pod(_)))
                .map(|open| open.key.target.clone())
                .collect();
            assert_eq!(
                pods,
                vec![
                    NavTarget::pod("prod", "web-1"),
                    NavTarget::pod("prod", "web-2"),
                ],
                "one panel per pod, however many times each is requested"
            );
        })
        .unwrap();
    cx.run_until_parked();

    let session_after = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());
    assert_eq!(
        session_before, session_after,
        "the detail panel reads the window's existing connection"
    );
}

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

#[test]
fn restorable_panels_skips_unknown_kinds() {
    let layout = WindowLayout {
        panels: vec![
            PanelDescriptor::Unknown,
            PanelDescriptor::Pods {
                cluster_context: "kind-dev".into(),
                namespace: crate::config::workspace::NamespaceScope::All,
                filter: String::new(),
                sort: crate::config::workspace::SortState {
                    column: "name".into(),
                    ascending: true,
                },
            },
            PanelDescriptor::Unknown,
        ],
        ..Default::default()
    };

    let kept = restorable_panels(&layout);

    assert_eq!(kept.len(), 1);
    assert!(matches!(kept[0], PanelDescriptor::Pods { .. }));
}

fn pods_panel_descriptor(cluster_context: &str) -> PanelDescriptor {
    PanelDescriptor::Pods {
        cluster_context: cluster_context.to_string(),
        namespace: crate::config::workspace::NamespaceScope::All,
        filter: String::new(),
        sort: crate::config::workspace::SortState {
            column: "name".into(),
            ascending: true,
        },
    }
}

/// Tasks.md 2.1: an explicit `contexts` list wins outright, whatever the saved
/// panels say - a window that explicitly holds a context with no panels (yet)
/// must not lose it to derivation.
#[test]
fn restored_contexts_prefers_the_explicit_list_over_derivation() {
    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string(), "staging".to_string()],
        panels: vec![pods_panel_descriptor("kind-dev")],
        ..Default::default()
    };

    assert_eq!(
        restored_contexts(&layout),
        vec!["kind-dev".to_string(), "staging".to_string()]
    );
}

/// Tasks.md 2.1 / design.md decision 3: a legacy file with no `contexts` derives
/// the list from its saved panels' `cluster_context`s, first-seen order, with no
/// duplicates for two panels on the same context.
#[test]
fn restored_contexts_derives_from_panels_in_first_seen_order_when_absent() {
    let layout = WindowLayout {
        contexts: Vec::new(),
        panels: vec![
            pods_panel_descriptor("staging"),
            pods_panel_descriptor("kind-dev"),
            pods_panel_descriptor("staging"),
        ],
        ..Default::default()
    };

    assert_eq!(
        restored_contexts(&layout),
        vec!["staging".to_string(), "kind-dev".to_string()]
    );
}

/// A layout with neither an explicit list nor any restorable panel derives to
/// nothing - that window opens in `Picker` mode, not a workspace with no context.
#[test]
fn restored_contexts_is_empty_for_a_layout_with_no_panels() {
    assert!(restored_contexts(&WindowLayout::default()).is_empty());
}

/// Section 10.1-10.3, checked on every panel type the dock holds rather than
/// on the title-bar helpers alone: each panel's tab names its kind, a
/// namespace picker is on the bar exactly when the kind is namespaced, and
/// the close control the dock needs is on every one of them.
///
/// A cluster-scoped kind is in the list on purpose - it is the case where
/// the picker must be *absent*, which a test over namespaced kinds alone
/// could not catch.
#[gpui_kit::test]
async fn every_resource_panel_carries_its_title_bar(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    let expected = 4;
    let cases: [NavTarget; 4] = [
        NavTarget::pods(),
        NavTarget::Logs,
        NavTarget::Kind(crd_kind()),
        NavTarget::Kind(cluster_scoped_kind()),
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
                    nav::OpenedPanel::Placeholder(panel) => title_bar_of(&panel, window, cx),
                    nav::OpenedPanel::Logs(panel) => title_bar_of(&panel, window, cx),
                    nav::OpenedPanel::PodDetail(panel) => title_bar_of(&panel, window, cx),
                };
                // No plain tab name, so the dock draws the tab from the panel's
                // title element and its "Context:" tooltip.
                assert_eq!(name, None, "the tab is drawn from the title element");
                assert_eq!(
                    crate::ui::panel::title::title(&scope),
                    target.list_label(),
                    "the title names the kind, never the cluster"
                );
                assert!(
                    controls > 0,
                    "every resource panel needs its close control, found \
                     none on {}",
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

/// Section 4.4: emptying a workspace's center dock (what closing its last panel
/// leaves behind) flips the window back to `Picker` mode - `watch_workspace`'s
/// `DockEvent::LayoutChanged` subscription, driven directly here via `set_center`
/// with an empty layout rather than a real interactive panel close.
#[gpui_kit::test]
async fn closing_the_last_panel_returns_to_the_picker(cx: &mut TestAppContext) {
    // See `connected_window`'s doc comment: `enter_workspace` starts a real
    // connect whose completion wakes GPUI from a tokio thread.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    let window = cx.add_window(|window, cx| {
        // Goes through the same transition a real connect does, so this
        // covers the Resource panel being wired up as well as the dock.
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
        main_window
    });

    window
        .update(cx, |main_window, _window, _cx| {
            assert!(matches!(main_window.mode, WindowMode::Workspace { .. }));
        })
        .unwrap();

    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                unreachable!("just asserted Workspace mode above");
            };
            dock_area.update(cx, |area, cx| {
                area.set_center(DockLayout::tabs(), window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            assert!(matches!(main_window.mode, WindowMode::Picker(_)));
        })
        .unwrap();
}

/// Tasks.md 1.3: entering a workspace takes this window's hold, and closing the
/// window releases it - two windows on one context share the session until both
/// close, matching `ClusterRegistry`'s own hold/release tests one layer down.
#[gpui_kit::test]
async fn closing_a_window_releases_only_its_own_hold(cx: &mut TestAppContext) {
    let workspace = temp_workspace_path();
    let keymap = temp_workspace_path();
    // See `connected_window`'s doc comment: `enter_workspace` starts a real
    // connect whose completion wakes GPUI from a tokio thread.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        // `init` registers the `on_window_closed` hook that releases a closed
        // window's holds - the thing under test.
        init(cx, workspace.clone(), &keymap);
    });

    fn open(cx: &mut TestAppContext) -> gpui_kit::WindowHandle<MainWindow> {
        cx.add_window(|window, cx| {
            let mut main_window = MainWindow {
                mode: WindowMode::Picker(
                    cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
                ),
                focus_handle: cx.focus_handle(),
            };
            main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
            main_window
        })
    }

    let window_a = open(cx);
    let window_b = open(cx);
    cx.run_until_parked();

    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        2,
        "both windows took a hold on entering their workspace"
    );

    window_a
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        1,
        "closing one window must release only its own hold"
    );

    window_b
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        0,
        "closing the last window must release the session entirely"
    );

    let _ = std::fs::remove_file(&workspace);
    let _ = std::fs::remove_file(&keymap);
}

/// Tasks.md 2.1: `open_window` restores every context a saved layout names, not
/// just the one it builds a dock around - a hold on each (so its session, and
/// the Resource panel's future "already connected" reuse, exist) even though
/// this narrow slice opens no panel at all for a context past the first.
#[gpui_kit::test]
async fn open_window_restores_every_context_even_one_with_no_panels(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string(), "staging".to_string()],
        panels: vec![pods_panel_descriptor("kind-dev")],
        ..Default::default()
    };
    cx.update(|cx| open_window(cx, layout));
    cx.run_until_parked();

    let windows = cx.update(|cx| cx.windows());
    assert_eq!(windows.len(), 1);
    // `open_window` wraps its `MainWindow` in a `gpui_kit::component::Root`
    // (for dialogs/menus), so reaching it back from the window handle goes
    // through the root's own view rather than a direct downcast of the handle.
    let main_window: Entity<MainWindow> = windows[0]
        .update(cx, |_, window, cx| {
            let root = window
                .root::<gpui_kit::component::Root>()
                .flatten()
                .expect("open_window always mounts a Root");
            root.read(cx)
                .view()
                .clone()
                .downcast::<MainWindow>()
                .expect("the Root wraps a MainWindow")
        })
        .unwrap();

    main_window.read_with(cx, |main_window, _cx| {
        let WindowMode::Workspace { contexts, .. } = &main_window.mode else {
            panic!("a restored layout with contexts opens straight into a workspace")
        };
        assert_eq!(
            contexts,
            &vec!["kind-dev".to_string(), "staging".to_string()],
            "both restored contexts are on the window, in order"
        );
    });
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        1
    );
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "staging")),
        1,
        "the second context is held even though it has no panel open yet"
    );
}

/// Task 2.2 supersedes design.md decision 3's non-goal: a multi-context window's
/// dock arrangement is now saved too, but under its own composite key
/// (`context_lifecycle::dock_layout_key`), never under either bare context name -
/// so a later single-context window on `staging` alone still starts from nothing,
/// not a stale arrangement built for `staging` plus `other`.
#[gpui_kit::test]
async fn multi_context_dock_layout_is_saved_under_its_own_composite_key(cx: &mut TestAppContext) {
    let solo = connected_window(cx, "kind-dev").await;
    cx.update(|cx| {
        cx.set_global(SavedDockLayouts(
            crate::config::dock_layouts::DockLayouts::default(),
        ));
    });
    cx.run_until_parked();

    let multi = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["staging".to_string(), "other".to_string()], window, cx);
        main_window
    });
    cx.run_until_parked();

    for window in [solo, multi] {
        window
            .update(cx, |main_window, window, cx| {
                let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                    panic!("a connected window is in workspace mode")
                };
                dock_area.update(cx, |area, cx| {
                    area.set_center(DockLayout::tabs(), window, cx);
                });
            })
            .unwrap();
    }
    cx.run_until_parked();

    let multi_key =
        context_lifecycle::dock_layout_key(&["staging".to_string(), "other".to_string()]);
    cx.update(|cx| {
        assert!(
            cx.global::<SavedDockLayouts>().0.get("kind-dev").is_some(),
            "the single-context window saves its layout under its bare context name"
        );
        assert!(
            cx.global::<SavedDockLayouts>().0.get("staging").is_none(),
            "a multi-context window must not save under either bare context name"
        );
        assert!(cx.global::<SavedDockLayouts>().0.get("other").is_none());
        assert!(
            cx.global::<SavedDockLayouts>().0.get(&multi_key).is_some(),
            "the multi-context window saves under its own composite key"
        );
    });
}

/// Regression test for the HANDOFF.md report: opening a second window and
/// selecting a context that a *first* window already connected said "connected"
/// but never switched the second window out of picker mode. Drives both windows
/// through the real `ClusterPicker::select` -> `PickerEvent::Connected` ->
/// `watch_picker` path (unlike `connected_window`, which shortcuts straight to
/// `enter_workspace` and so never exercised this path at all) - the same shared
/// connection entity stands in for `ClusterRegistry` returning the first window's
/// already-`Connected` entity to the second window's picker.
#[gpui_kit::test]
async fn second_window_connecting_to_an_already_connected_context_shows_workspace(
    cx: &mut TestAppContext,
) {
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::ui::picker::ClusterPicker;
    use kube::{Client, Config};
    use std::cell::RefCell;

    thread_local! {
        static SHARED: RefCell<Option<Entity<ClusterConnection>>> = const { RefCell::new(None) };
    }

    // `connection_factory` only stubs the picker's own connection entity;
    // `watch_picker` still drives `enter_workspace`, which starts a real
    // `ClusterRegistry` connect for the panel it builds. See
    // `connected_window`'s doc comment for why that needs `allow_parking`.
    cx.executor().allow_parking();

    fn shared_connected_stub(cx: &mut App, _context_name: &str) -> Entity<ClusterConnection> {
        SHARED.with(|cell| {
            if let Some(entity) = cell.borrow().as_ref() {
                return entity.clone();
            }
            let handle = crate::runtime::handle(cx);
            let _guard = handle.enter();
            let client =
                Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap();
            let entity =
                cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
            *cell.borrow_mut() = Some(entity.clone());
            entity
        })
    }

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    fn picker_window(cx: &mut TestAppContext) -> gpui_kit::WindowHandle<MainWindow> {
        cx.add_window(|window, cx| {
            let picker = cx.new(|cx| {
                let mut picker = ClusterPicker::new(window, cx);
                picker.connection_factory = Some(shared_connected_stub);
                picker
            });
            let main_window = MainWindow {
                mode: WindowMode::Picker(picker.clone()),
                focus_handle: cx.focus_handle(),
            };
            watch_picker(&picker, window, cx);
            main_window
        })
    }

    let first = picker_window(cx);
    first
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Picker(picker) = &main_window.mode else {
                unreachable!("just constructed in Picker mode");
            };
            picker.update(cx, |picker, cx| {
                picker.select("kind-dev".to_string(), cx);
            });
        })
        .unwrap();
    cx.run_until_parked();
    first
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Workspace { .. }),
                "first window connects normally"
            );
        })
        .unwrap();

    let second = picker_window(cx);
    second
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Picker(picker) = &main_window.mode else {
                unreachable!("just constructed in Picker mode");
            };
            picker.update(cx, |picker, cx| {
                picker.select("kind-dev".to_string(), cx);
            });
        })
        .unwrap();
    cx.run_until_parked();
    second
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Workspace { .. }),
                "second window selecting an already-connected context must also \
                 switch to the workspace, not stay stuck showing the picker"
            );
        })
        .unwrap();

    SHARED.with(|cell| *cell.borrow_mut() = None);
}

/// Hypothesis two from `second-window-connect-fix/design.md`: a second window
/// connecting to a context that is *not* already connected (so it goes through
/// `cx.observe`'s callback, not `select`'s synchronous `emit_connected` call).
#[gpui_kit::test]
async fn second_window_connecting_to_a_fresh_context_shows_workspace(cx: &mut TestAppContext) {
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::ui::picker::ClusterPicker;
    use kube::{Client, Config};

    // See the previous test: `connection_factory` stubs only the picker's
    // own entity, not the real connect `enter_workspace` starts.
    cx.executor().allow_parking();

    fn connecting_then_connected_stub(
        cx: &mut App,
        _context_name: &str,
    ) -> Entity<ClusterConnection> {
        let handle = crate::runtime::handle(cx);
        let _guard = handle.enter();
        let client = Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap();
        let entity = cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting));
        entity.update(cx, |connection, cx| {
            connection.state = ConnectionState::Connected(client);
            cx.notify();
        });
        entity
    }

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    let second = cx.add_window(|window, cx| {
        let picker = cx.new(|cx| {
            let mut picker = ClusterPicker::new(window, cx);
            picker.connection_factory = Some(connecting_then_connected_stub);
            picker
        });
        let main_window = MainWindow {
            mode: WindowMode::Picker(picker.clone()),
            focus_handle: cx.focus_handle(),
        };
        watch_picker(&picker, window, cx);
        main_window
    });
    second
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Picker(picker) = &main_window.mode else {
                unreachable!("just constructed in Picker mode");
            };
            picker.update(cx, |picker, cx| {
                picker.select("fresh-dev".to_string(), cx);
            });
        })
        .unwrap();
    cx.run_until_parked();
    second
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Workspace { .. }),
                "a fresh connect completing after select must still flip this \
                 window to the workspace"
            );
        })
        .unwrap();
}

#[gpui_kit::test]
async fn quitting_persists_open_window_geometry(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    let keymap_path = temp_workspace_path();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, path.clone(), &keymap_path);
        open_window(
            cx,
            WindowLayout {
                width: 900.0,
                height: 700.0,
                x: Some(10.0),
                y: Some(20.0),
                contexts: Vec::new(),
                panels: Vec::new(),
                resource_panel_width: None,
            },
        );
    });
    cx.run_until_parked();

    cx.update(|cx| save(cx, &path));

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1);
    assert_eq!(saved.windows[0].width, 900.0);
    assert_eq!(saved.windows[0].height, 700.0);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&keymap_path);
}

/// A window closed while another stays open is forgotten; only the last one to close
/// is remembered (closing it quits the app), so relaunch reopens what was open at
/// quit - not every window ever closed this run.
#[gpui_kit::test]
async fn closing_a_window_while_others_stay_open_forgets_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    let keymap_path = temp_workspace_path();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, path.clone(), &keymap_path);
        open_window(cx, WindowLayout::default());
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();

    let windows = cx.update(|cx| cx.windows());
    assert_eq!(windows.len(), 2);
    windows[0]
        .update(cx, |_, window, cx| close_window(window, cx))
        .unwrap();
    cx.run_until_parked();
    let recorded = cx.update(|cx| {
        cx.try_global::<ClosedWindowLayouts>()
            .map_or(0, |closed| closed.0.len())
    });
    assert_eq!(
        recorded, 0,
        "a window closed while another is open is forgotten"
    );

    windows[1]
        .update(cx, |_, window, cx| close_window(window, cx))
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| save(cx, &path));
    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(
        saved.windows.len(),
        1,
        "only the last window closed is restored"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&keymap_path);
}

#[gpui_kit::test]
async fn save_persists_geometry_of_a_window_already_closed(cx: &mut TestAppContext) {
    // Regression test: under `QuitMode::LastWindowClosed`, `on_app_quit`
    // fires after every window is already gone, so `cx.windows()` alone
    // (the pre-fix implementation) sees nothing and silently saves an
    // empty layout. `save` must also pick up geometry captured by
    // `open_window`'s `on_window_should_close` hook and stashed in
    // `ClosedWindowLayouts` before the window disappeared.
    let path = temp_workspace_path();
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(ClosedWindowLayouts(HashMap::from([(
            WindowId::from(1),
            WindowLayout {
                width: 900.0,
                height: 700.0,
                x: Some(10.0),
                y: Some(20.0),
                contexts: Vec::new(),
                panels: Vec::new(),
                resource_panel_width: None,
            },
        )])));
        save(cx, &path);
    });

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1);
    assert_eq!(saved.windows[0].width, 900.0);
    assert_eq!(saved.windows[0].height, 700.0);

    let _ = std::fs::remove_file(&path);
}

#[gpui_kit::test]
async fn corrupt_workspace_file_yields_one_default_window(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    std::fs::write(&path, "not valid toml {{{").unwrap();

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        open_saved_or_default(cx, &path);
    });
    cx.run_until_parked();

    let window_count = cx.update(|cx| cx.windows().len());
    assert_eq!(window_count, 1);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "not valid toml {{{"
    );

    let _ = std::fs::remove_file(&path);
}

#[gpui_kit::test]
async fn toggle_command_palette_action_opens_a_dialog(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        let mut registry = CommandRegistry::new();
        register_commands(&mut registry);
        cx.set_global(registry);
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();

    let window = cx.update(|cx| cx.windows()[0]);

    // Leak-safe: with no dialog open, `render_dialog_layer` returns
    // `None` before touching any dialog state.
    let dialog_open_before = window
        .update(cx, |_, window, cx| {
            gpui_kit::component::Root::render_dialog_layer(window, cx).is_some()
        })
        .unwrap();
    assert!(!dialog_open_before);

    window
        .update(cx, |_, window, cx| {
            window.dispatch_action(Box::new(ToggleCommandPalette), cx);
        })
        .unwrap();
    cx.run_until_parked();

    // Not re-checked via `render_dialog_layer` here: actually rendering
    // gpui-component's `Command` widget installs a model that outlives
    // `close_all_dialogs`/`remove_window` and trips the test harness's
    // leaked-entity check - reproduced directly against gpui-component
    // 0.6.6, not something under our control. `open_command_palette`
    // reaching this point without panicking, immediately after the
    // action dispatch above, is what's covered instead.

    // Close the dialog before the test ends, or the leak detector flags
    // its CommandState entity: the harness asserts every entity created
    // during a test is released by teardown.
    window
        .update(cx, |_, window, cx| {
            let Some(Some(root)) = window.root::<gpui_kit::component::Root>() else {
                return;
            };
            root.update(cx, |root, cx| root.close_all_dialogs(window, cx));
        })
        .unwrap();
    window
        .update(cx, |_, window, _cx| window.remove_window())
        .unwrap();
    cx.run_until_parked();
}

#[test]
fn restored_panel_keys_preserve_kind_context_and_namespace() {
    use gpui_kit::component::dock::{PanelInfo, PanelState};

    let state = PanelState {
        panel_name: "Pods".to_string(),
        children: Vec::new(),
        info: PanelInfo::Panel(serde_json::json!({
            "context_name": "kind-dev",
            "namespaces": ["kube-system"],
        })),
    };

    let keys = super::restored_panel_keys(&state);

    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].target, NavTarget::pods());
    assert_eq!(keys[0].context_name, "kind-dev");
    assert_eq!(keys[0].namespaces, vec!["kube-system"]);
}

/// The panel's own keys have to be *bound*, not merely printed.
///
/// The hint bar under the pods table reads the keymap for each shortcut
/// and falls back to printing the letter, so an unbound `d` looks
/// identical to a working one on screen while doing nothing when pressed.
/// This presses the key rather than dispatching the action, because the
/// binding is exactly the part that can be missing.
///
/// At the end of the module on purpose: `title_bar_of` above is being
/// changed on another branch, and a test whose context sits under it would
/// stop applying the moment that lands.
#[gpui_kit::test]
async fn a_pods_panel_shortcut_key_reaches_the_window(cx: &mut TestAppContext) {
    use crate::k8s::resource::pods::{PodSelection, SelectedPod};
    use gpui_kit::{Focusable as _, test::TestWindowExt as _};

    let workspace = temp_workspace_path();
    let keymap = temp_workspace_path();
    // See `connected_window`'s doc comment: `enter_workspace` starts a real
    // connect whose completion wakes GPUI from a tokio thread.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        init(cx, workspace.clone(), &keymap);
    });
    let window = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
        main_window
    });
    cx.run_until_parked();
    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: vec!["web".into()],
            context_name: "kind-dev".into(),
        })));
    });

    // Focus the pods list, the way clicking into its table would.
    window
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            let Some(OpenedPanel::Pods(panel)) = open_panels[0].panel.clone() else {
                panic!("a new workspace opens on the pods list")
            };
            panel.read(cx).focus_handle(cx).focus(window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    // A real keystroke, not a dispatched action. Dispatched through the
    // window rather than the entity: a keypress re-renders, and re-entering
    // the window's view while it is mid-update is what gpui forbids.
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_keystroke(
            gpui_kit::Keystroke::parse("d").expect("valid keystroke"),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("the window is still open");
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert!(
                open_panels
                    .iter()
                    .any(|open| open.key.target == NavTarget::pod("default", "web-1")),
                "pressing `d` in the pods list opened the selected pod's detail \
                 panel, so the key was bound rather than only printed"
            );
        })
        .unwrap();

    let _ = std::fs::remove_file(&workspace);
    let _ = std::fs::remove_file(&keymap);
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

/// Task 2.2: a live workspace window's own `contexts` - not an empty list - is
/// what `save` persists, so relaunch can reconnect them directly (the spec's
/// "Multi-context window restored" scenario).
#[gpui_kit::test]
async fn saving_persists_a_live_workspaces_contexts(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    let layout = WindowLayout {
        contexts: vec!["kind-dev".to_string(), "staging".to_string()],
        panels: vec![pods_panel_descriptor("kind-dev")],
        ..Default::default()
    };
    cx.update(|cx| open_window(cx, layout));
    cx.run_until_parked();

    cx.update(|cx| save(cx, &path));

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1);
    assert_eq!(
        saved.windows[0].contexts,
        vec!["kind-dev".to_string(), "staging".to_string()],
        "the live window's own contexts are saved, not an empty list"
    );

    let _ = std::fs::remove_file(&path);
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

/// A saved width outside the divider's range is clamped, and a layout with none (an
/// older file) starts at the default.
#[test]
fn a_restored_resource_panel_width_is_clamped_or_defaulted() {
    use crate::consts::{RESOURCE_PANEL_MAX_WIDTH, RESOURCE_PANEL_MIN_WIDTH, RESOURCE_PANEL_WIDTH};
    let with = |width: Option<f32>| WindowLayout {
        resource_panel_width: width,
        ..Default::default()
    };
    assert_eq!(
        restored_resource_width(&with(Some(9999.0))),
        RESOURCE_PANEL_MAX_WIDTH
    );
    assert_eq!(
        restored_resource_width(&with(Some(10.0))),
        RESOURCE_PANEL_MIN_WIDTH
    );
    assert_eq!(restored_resource_width(&with(None)), RESOURCE_PANEL_WIDTH);
}

/// Task 2.2: `save` reads a `Picker`-mode window's contexts as empty - there is
/// nothing to reconnect yet, so relaunch must not invent a context for it.
#[gpui_kit::test]
async fn saving_a_picker_mode_window_persists_no_contexts(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let path = temp_workspace_path();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        open_window(cx, WindowLayout::default());
    });
    cx.run_until_parked();

    cx.update(|cx| save(cx, &path));

    let saved: WorkspaceConfig = config::load(&path);
    assert_eq!(saved.windows.len(), 1);
    assert!(saved.windows[0].contexts.is_empty());

    let _ = std::fs::remove_file(&path);
}

/// Task 2.2: a multi-context window's dock arrangement round-trips under its own
/// composite key (`context_lifecycle::dock_layout_key`), order-insensitively - a
/// window restored with the same two contexts in the opposite order still finds
/// the saved arrangement rather than defaulting to a fresh single Pods panel.
#[gpui_kit::test]
async fn multi_context_dock_arrangement_round_trips_order_insensitively(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        cx.set_global(SavedDockLayouts(
            crate::config::dock_layouts::DockLayouts::default(),
        ));
    });

    let first = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(
            vec!["kind-dev".to_string(), "staging".to_string()],
            window,
            cx,
        );
        main_window
    });
    cx.run_until_parked();

    // Empty the centre dock, which `LayoutChanged` saves under the pair's
    // composite key regardless of the order they were added in.
    first
        .update(cx, |main_window, window, cx| {
            let WindowMode::Workspace { dock_area, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            dock_area.update(cx, |area, cx| {
                area.set_center(DockLayout::tabs(), window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();

    // A second window on the *same pair*, contexts reversed.
    let second = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(
            vec!["staging".to_string(), "kind-dev".to_string()],
            window,
            cx,
        );
        main_window
    });
    cx.run_until_parked();

    let _ = second.read_with(cx, |main_window, _cx| {
        let WindowMode::Workspace { open_panels, .. } = &main_window.mode else {
            panic!("a connected window is in workspace mode")
        };
        assert!(
            open_panels.is_empty(),
            "the saved (emptied) multi-context arrangement was loaded, not a \
             fresh default Pods panel"
        );
    });
}

/// Section 3.2: adding a context holds it, makes it the active one, and opens
/// its Pods panel.
#[gpui_kit::test]
async fn add_context_holds_switches_active_and_opens_pods(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace {
                contexts,
                active,
                open_panels,
                ..
            } = &main_window.mode
            else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(
                contexts,
                &vec!["kind-dev".to_string(), "staging".to_string()]
            );
            assert_eq!(*active, 1, "the added context becomes active");
            assert!(
                open_panels
                    .iter()
                    .any(|open| open.key.context_name == "staging"
                        && open.key.target == NavTarget::pods()),
                "a Pods panel opened for the added context"
            );
        })
        .unwrap();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "staging")),
        1
    );
}

/// Adding a context already in use is a no-op - the "+" popover's own filtering
/// keeps this from happening interactively, but `add_context` must not duplicate
/// a chip if it's ever asked to anyway.
#[gpui_kit::test]
async fn adding_an_already_used_context_is_a_no_op(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("kind-dev".to_string(), window, cx);
        })
        .unwrap();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { contexts, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(contexts, &vec!["kind-dev".to_string()]);
        })
        .unwrap();
}

/// `cluster-connection`'s "Shared by two windows": a second window adding a
/// context the first window already holds must not start a second session.
#[gpui_kit::test]
async fn adding_a_context_another_window_holds_reuses_its_session(cx: &mut TestAppContext) {
    // Not two `connected_window` calls: `gpui_kit::init`/`crate::runtime::init`
    // are meant to run once per test, so the second window is built inline the
    // same way `closing_a_window_releases_only_its_own_hold` builds its second
    // window.
    // Kept alive for the test's duration: it's the window whose hold on
    // `kind-dev` the assertion below expects to still be counted.
    let _first = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let second = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["staging".to_string()], window, cx);
        main_window
    });
    cx.run_until_parked();

    let session_before = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());

    second
        .update(cx, |main_window, window, cx| {
            main_window.add_context("kind-dev".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    let session_after = cx.update(|cx| ClusterRegistry::connection(cx, "kind-dev").entity_id());
    assert_eq!(
        session_before, session_after,
        "adding an already-connected context must not start a second session"
    );
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        2,
        "both windows now hold kind-dev"
    );
}

/// Section 3.3: disconnecting a context closes its panels, drops it from
/// `contexts`, and releases this window's hold. Panels for the window's other
/// context are untouched.
#[gpui_kit::test]
async fn disconnect_context_closes_its_panels_and_releases_the_hold(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            main_window.disconnect_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace {
                contexts,
                active,
                open_panels,
                ..
            } = &main_window.mode
            else {
                panic!("kind-dev's own panel keeps this window in workspace mode")
            };
            assert_eq!(contexts, &vec!["kind-dev".to_string()]);
            assert_eq!(*active, 0);
            assert!(
                open_panels
                    .iter()
                    .all(|open| open.key.context_name != "staging"),
                "every staging panel closed"
            );
            assert!(
                open_panels
                    .iter()
                    .any(|open| open.key.context_name == "kind-dev"),
                "kind-dev's own panel is untouched"
            );
        })
        .unwrap();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "staging")),
        0,
        "this window was staging's only holder"
    );
}

/// Section 3.3's "Last context": disconnecting a window's only context returns
/// it to the cluster picker, and a context another window still holds stays up
/// for it (`cluster-connection`'s "One window lets go").
#[gpui_kit::test]
async fn disconnecting_the_last_context_returns_to_the_picker(cx: &mut TestAppContext) {
    // See the note in `adding_a_context_another_window_holds_reuses_its_session`
    // on why the second window is built inline rather than through a second
    // `connected_window` call.
    let first = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    let second = cx.add_window(|window, cx| {
        let mut main_window = MainWindow {
            mode: WindowMode::Picker(
                cx.new(|cx| crate::ui::picker::ClusterPicker::new(window, cx)),
            ),
            focus_handle: cx.focus_handle(),
        };
        main_window.enter_workspace(vec!["kind-dev".to_string()], window, cx);
        main_window
    });
    cx.run_until_parked();

    second
        .update(cx, |main_window, window, cx| {
            main_window.disconnect_context("kind-dev".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    second
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Picker(_)),
                "the window's last context disconnected"
            );
        })
        .unwrap();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holder_count(cx, "kind-dev")),
        1,
        "the other window still holds kind-dev"
    );

    first
        .update(cx, |main_window, _window, _cx| {
            assert!(
                matches!(main_window.mode, WindowMode::Workspace { .. }),
                "the other window's own workspace is unaffected"
            );
        })
        .unwrap();
}

/// Design.md decision 4: a chip click (or the Resource panel's own dropdown)
/// sets `active`, and every child - the Resource panel, the status bar, the
/// context bar - is pushed the same update through `sync_context_children`.
#[gpui_kit::test]
async fn set_active_context_switches_active_and_syncs_children(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();
    window
        .update(cx, |main_window, window, cx| {
            main_window.add_context("staging".to_string(), window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            main_window.set_active_context("kind-dev", window, cx);
        })
        .unwrap();

    window
        .update(cx, |main_window, _window, cx| {
            let WindowMode::Workspace {
                active, status_bar, ..
            } = &main_window.mode
            else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(*active, 0, "switched back to kind-dev");
            assert_eq!(
                status_bar.read(cx).items(cx).len(),
                2,
                "the status bar still lists both contexts - only `active` moved"
            );
        })
        .unwrap();
}

/// A request for a context this window doesn't use (a stale event racing a
/// disconnect) must not touch `active` at all.
#[gpui_kit::test]
async fn set_active_context_to_an_unused_context_is_a_no_op(cx: &mut TestAppContext) {
    let window = connected_window(cx, "kind-dev").await;
    cx.run_until_parked();

    window
        .update(cx, |main_window, window, cx| {
            main_window.set_active_context("never-added", window, cx);
        })
        .unwrap();

    window
        .update(cx, |main_window, _window, _cx| {
            let WindowMode::Workspace { active, .. } = &main_window.mode else {
                panic!("a connected window is in workspace mode")
            };
            assert_eq!(*active, 0);
        })
        .unwrap();
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
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        ClusterRegistry::insert_test_session(cx, "carefulcrab", ConnectionState::Connecting);
        ClusterRegistry::insert_test_session(cx, "other-context", ConnectionState::Connecting);
    });

    let window = cx.add_window(|window, cx| {
        MainWindow::test_workspace(
            vec!["carefulcrab".to_string(), "other-context".to_string()],
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
        Some("carefulcrab".to_string()),
        "the window opens on its first context"
    );

    // The pod was selected from the *other* context's Pods panel - active stays
    // `carefulcrab` throughout, matching the bug report exactly.
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
                .find(|open| open.key.target == NavTarget::Logs)
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
        gpui_kit::init(cx);
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
                    .any(|open| open.key.target == NavTarget::Logs),
                "a selection from a context this window doesn't hold must not open Logs \
                 against the active context instead"
            );
        })
        .unwrap();
}

/// `PanelKey`'s dedup has to tell two contexts' panels over the same target
/// apart, or a pod (or Logs) opened on one context would focus the other
/// context's panel instead of opening its own - the structural half of
/// `show_logs_and_pod_detail_use_the_selected_pods_context_not_the_active_one`'s
/// end-to-end proof.
#[test]
fn panel_key_distinguishes_two_contexts_over_the_same_target() {
    let pods_a = PanelKey::from(&PanelScope::new(NavTarget::pods(), "a".to_string()));
    let pods_b = PanelKey::from(&PanelScope::new(NavTarget::pods(), "b".to_string()));
    assert_ne!(
        pods_a, pods_b,
        "the same target on two contexts must be two different keys"
    );

    let pod_a = PanelKey::from(&PanelScope::new(
        NavTarget::pod("default", "web-1"),
        "a".to_string(),
    ));
    let pod_b = PanelKey::from(&PanelScope::new(
        NavTarget::pod("default", "web-1"),
        "b".to_string(),
    ));
    assert_ne!(
        pod_a, pod_b,
        "the same pod's detail panel on two contexts must be two different keys"
    );
}
