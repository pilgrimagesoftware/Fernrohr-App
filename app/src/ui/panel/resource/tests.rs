// Named imports rather than `use super::*;` - see the comment above `mod tests;` in
// `resource.rs` for why: a glob here re-imports `gpui_kit::*`'s huge surface a second
// time and blows this toolchain's macro-expansion budget once a `#[gpui_kit::test]`
// item sits in the same module.
use super::{ResourcePanel, ResourceState};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::ui::nav::NavTarget;
use gpui_kit::{AppContext as _, TestAppContext, WindowHandle};
use kube::core::GroupVersionKind;

// `resource-panel-grouping`: section 1/2/3's pure category/grouping/filter
// tests need no window (`logic`); section 2.2/2.3's collapse behavior and
// section 3's live filter box each need a real panel (`grouping`, `filter`);
// section 4's keyboard route needs real keystrokes (`keyboard_panel`).
// Children of this module rather than this file's own line count, per
// `.claude/rules/rust-structure.md`.
mod filter;
mod grouping;
mod keyboard_panel;
mod logic;
mod spacing;
mod subgroup_keys;
mod subgroups;

fn kind(group: &str, kind: &str) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: format!("{}s", kind.to_lowercase()),
        namespaced: true,
        verbs: Default::default(),
    }
}

/// A panel wired to a stub connection instead of the real registry - see
/// [`ResourcePanel::with_connection`]. The stub stays non-`Connected`, so
/// nothing spawns a real connect or discovery task.
fn stub_panel(cx: &mut TestAppContext) -> WindowHandle<ResourcePanel> {
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    cx.add_window(|window, cx| {
        ResourcePanel::with_connection(
            "kind-dev".to_string(),
            vec!["kind-dev".to_string()],
            connection.clone(),
            window,
            cx,
        )
    })
}

#[gpui_kit::test]
async fn cluster_dropdown_requires_multiple_connections(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let window = cx.add_window(|window, cx| {
        ResourcePanel::with_contexts(
            "kind-dev".to_string(),
            vec!["kind-dev".to_string(), "kind-staging".to_string()],
            connection,
            window,
            cx,
        )
    });

    window
        .update(cx, |panel, _window, _cx| {
            assert!(panel.shows_cluster_dropdown());
        })
        .unwrap();
}

/// Section 8.1: what the panel lists is exactly what discovery returned -
/// one row per discovered kind, with the CRD kinds among them. Driving the
/// loaded state directly is the seam: the discovery call itself is covered
/// by `cluster::discovery`'s fixture tests.
#[gpui_kit::test]
async fn a_loaded_panel_lists_every_discovered_kind_including_crds(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);

    let discovered = vec![
        kind("", "Pod"),
        kind("", "Service"),
        kind("apps", "Deployment"),
        kind("ferns.example.com", "Fern"),
    ];
    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Loaded(discovered.clone());
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |panel, _window, _cx| {
            let listed = panel.kinds().expect("the panel has loaded kinds");
            assert_eq!(listed.len(), discovered.len(), "one row per kind");

            let labels: Vec<String> = listed.iter().map(DiscoveredKind::label).collect();
            assert!(labels.contains(&"Pod".to_string()));
            assert!(labels.contains(&"Deployment · apps".to_string()));
            assert!(
                labels.contains(&"Fern · ferns.example.com".to_string()),
                "the CRD gets a row like any other kind: {labels:?}"
            );
        })
        .unwrap();
}

/// Section 8.2: every listed kind is openable - the built-in Pod maps to the
/// Pods panel, and a CRD maps to a placeholder rather than to nothing. This
/// is the same `rows` mapping the sidebar's click handlers are built from.
#[gpui_kit::test]
async fn every_row_opens_a_panel_and_the_crd_gets_a_placeholder(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);

    let fern = kind("ferns.example.com", "Fern");
    let pod = DiscoveredKind::pods();
    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Loaded(vec![fern.clone(), pod.clone()]);
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |panel, _window, _cx| {
            let rows = panel.rows(&[fern.clone(), pod.clone()]);
            assert_eq!(rows.len(), 2, "one row per kind");

            for (label, target, active) in &rows {
                assert!(!label.is_empty(), "every row names the kind it opens");
                assert!(
                    !active,
                    "nothing is selected before the window opens a panel"
                );

                let opened = match target {
                    NavTarget::Kind(kind) => kind,
                    NavTarget::Logs => panic!("a discovered kind, not Logs"),
                    NavTarget::Pod(_) | NavTarget::Object(_) => {
                        panic!("a discovered kind, not one object's detail")
                    }
                };
                // Every row resolves to a kind `add_panel` opens a list for.
                let _ = opened;
            }

            let (_, fern_target, _) = &rows[0];
            assert!(
                !match fern_target {
                    NavTarget::Kind(kind) => kind,
                    NavTarget::Logs | NavTarget::Pod(_) | NavTarget::Object(_) => unreachable!(),
                }
                .is_core_pod()
            );
            let (_, pod_target, _) = &rows[1];
            assert!(
                match pod_target {
                    NavTarget::Kind(kind) => kind,
                    NavTarget::Logs | NavTarget::Pod(_) | NavTarget::Object(_) => unreachable!(),
                }
                .is_core_pod()
            );
        })
        .unwrap();
}

/// The window marks the open panel's row, so the list shows where the user
/// is. Only the selected kind is marked - the rest stay unselected.
#[gpui_kit::test]
async fn the_open_panels_row_is_the_only_one_marked(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);

    let kinds = vec![
        kind("", "Service"),
        DiscoveredKind::pods(),
        kind("ferns.example.com", "Fern"),
    ];
    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Loaded(kinds.clone());
            panel.set_selected(Some(NavTarget::pods()), cx);
        })
        .unwrap();

    window
        .update(cx, |panel, _window, _cx| {
            let rows = panel.rows(&kinds);
            let marked: Vec<&str> = rows
                .iter()
                .filter(|(_, _, active)| *active)
                .map(|(label, _, _)| label.as_str())
                .collect();
            assert_eq!(marked, vec!["Pod"]);
        })
        .unwrap();
}

/// Section 9.2: the row's context-menu "Open" and its double-click are
/// required to be equivalent. They are equivalent here by construction -
/// both closures capture the same `target` and call the same
/// `request_open` - so what this pins is that the shared path emits exactly
/// one request for the row's own kind, which is what a second selector
/// would otherwise turn into two panels.
#[gpui_kit::test]
async fn opening_a_row_emits_one_request_for_that_rows_kind(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);

    let fern = kind("ferns.example.com", "Fern");
    let opened: Vec<NavTarget> = Vec::new();
    let collected = std::rc::Rc::new(std::cell::RefCell::new(opened));

    window
        .update(cx, |panel, window, cx| {
            panel.state = ResourceState::Loaded(vec![fern.clone()]);
            let collected = collected.clone();
            let entity = cx.entity();
            cx.subscribe_in(
                &entity,
                window,
                move |_panel, _entity, event, _window, _cx| {
                    let super::ResourceEvent::Open(target) = event else {
                        return;
                    };
                    collected.borrow_mut().push(target.clone());
                },
            )
            .detach();
            cx.notify();
        })
        .unwrap();

    // Whatever selector fired - double-click or "Open" - it lands here.
    window
        .update(cx, |panel, _window, cx| {
            panel.request_open(NavTarget::Kind(fern.clone()), cx);
        })
        .unwrap();
    cx.run_until_parked();

    let opened = collected.borrow();
    assert_eq!(opened.len(), 1, "one request, so one panel");
    assert_eq!(opened[0], NavTarget::Kind(fern));
}

/// A cluster that reported nothing still renders the sidebar rather than
/// collapsing the window's left edge away.
#[gpui_kit::test]
async fn an_empty_discovery_still_leaves_a_panel(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);

    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Loaded(Vec::new());
            cx.notify();
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |panel, _window, _cx| {
            assert_eq!(panel.kinds(), Some(&[][..]));
        })
        .unwrap();
}

/// A discovery failure is reported rather than swallowed, and leaves the
/// panel with no list rather than a stale one.
#[gpui_kit::test]
async fn a_failed_discovery_is_reported_and_lists_nothing(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = stub_panel(cx);

    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Failed("connection refused".to_string());
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |panel, _window, _cx| {
            assert!(panel.kinds().is_none(), "no list on failure");
        })
        .unwrap();
}

/// `window-context-bar` design.md decision 4: switching to a different context
/// resets discovery state and reads the new context's connection through
/// `ClusterRegistry`, rather than keeping the old context's `Loaded` list around.
///
/// `staging`'s test session is seeded still-`Connecting`, not `Connected`, so the
/// switch itself is fully deterministic: `sync`'s guard leaves `state` at
/// `WaitingForConnection` rather than racing a real discovery attempt (against a
/// fake client, on a real tokio thread) against this test's own assertions - the
/// same hazard `connected_window`'s doc comment in `util/shell.rs` names for a
/// real `ClusterConnection::connect`. Flipping `staging` to `Connected` afterward
/// and checking `sync` reacts is the "observes the new connection" half, and it's
/// still deterministic: `cx.notify()` runs this panel's `cx.observe` callback
/// synchronously, the same guarantee `k8s::cluster::session`'s own
/// `pause_and_resume_each_notify_registry_observers` test relies on.
#[gpui_kit::test]
async fn set_active_context_to_a_different_context_reloads_from_its_connection(
    cx: &mut TestAppContext,
) {
    // Flipping `staging` to `Connected` below starts a real discovery request (a
    // tokio task) against a fake client - see `connected_window`'s doc comment in
    // `util/shell.rs` for why that needs this, same as every other test here that
    // drives a real connect or discovery.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let kind_dev =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let window = cx.add_window(|window, cx| {
        ResourcePanel::with_contexts(
            "kind-dev".to_string(),
            vec!["kind-dev".to_string(), "staging".to_string()],
            kind_dev,
            window,
            cx,
        )
    });
    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Loaded(vec![DiscoveredKind::pods()]);
            panel.set_selected(Some(NavTarget::pods()), cx);
        })
        .unwrap();

    let staging = cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connecting)
    });

    window
        .update(cx, |panel, _window, cx| {
            let contexts = panel.contexts.clone();
            panel.set_active_context("staging".to_string(), contexts, cx);
        })
        .unwrap();

    window
        .update(cx, |panel, _window, _cx| {
            assert_eq!(panel.context_name, "staging", "the shown cluster switched");
            assert!(
                panel.selected.is_none(),
                "the old selection belonged to kind-dev's kinds"
            );
            assert!(
                matches!(panel.state, ResourceState::WaitingForConnection),
                "kind-dev's stale Loaded list must not survive the switch"
            );
        })
        .unwrap();

    let client = {
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
    };
    cx.update(|cx| {
        staging.update(cx, |connection, cx| {
            connection.state = ConnectionState::Connected(client);
            cx.notify();
        });
    });

    window
        .update(cx, |panel, _window, _cx| {
            assert!(
                matches!(panel.state, ResourceState::Loading),
                "the panel is observing staging's connection, not kind-dev's"
            );
        })
        .unwrap();
}

/// Picking the context that's already active must not restart discovery - a
/// dropdown pick of the current cluster is a no-op, not a reload.
#[gpui_kit::test]
async fn set_active_context_to_the_current_context_does_not_reload(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let window = cx.add_window(|window, cx| {
        ResourcePanel::with_contexts(
            "kind-dev".to_string(),
            vec!["kind-dev".to_string(), "staging".to_string()],
            connection,
            window,
            cx,
        )
    });
    window
        .update(cx, |panel, _window, cx| {
            panel.state = ResourceState::Loaded(vec![DiscoveredKind::pods()]);
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |panel, _window, cx| {
            panel.set_active_context("kind-dev".to_string(), panel.contexts.clone(), cx);
        })
        .unwrap();

    window
        .update(cx, |panel, _window, _cx| {
            assert!(
                matches!(panel.state, ResourceState::Loaded(_)),
                "picking the already-active context must not reset discovery"
            );
        })
        .unwrap();
}
