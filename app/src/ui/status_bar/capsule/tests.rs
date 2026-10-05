//! The capsules' view-level tests, carried over from the context bar they replaced
//! (`window-context-bar` section 3, `toolbar-layout-with-gpui-kit` 1.1): what one
//! capsule reports ([`StatusItem`], via [`StatusBarView::items`]) and that a capsule
//! click reaches [`MainWindow::set_active_context`].
//!
//! Two scenarios this module deliberately does not cover, because they are
//! already proven elsewhere and repeating them here would only pin the same
//! assertion twice:
//! - The "+" popover excludes contexts already in use - `ClusterPicker::exclude`'s
//!   own behavior, covered in `ui/picker/tests.rs` (the only place its private
//!   `contexts` field is reachable without a real kubeconfig, which `ClusterPicker::new`
//!   otherwise reads for real).
//! - The Disconnect confirmation's exact wording (panel count, "N other windows")
//!   - `context_lifecycle::disconnect_confirmation_body`'s own tests in
//!     `util/context_lifecycle.rs` already exercise 0/1/n panels and 0/1/n other
//!     windows. `open_disconnect_dialog` only feeds it two already-tested numbers
//!     (`MainWindow::context_panel_count`, `ClusterRegistry::holder_count`).
//!
//! Not `use super::*;` - see the comment above `mod tests;` in `status_bar.rs`
//! for why a glob here crashes the compiler.
use crate::config::tunnels::{TunnelAuth, TunnelConfig};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::context_health::Severity;
use crate::k8s::cluster::health::HealthTransition;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::cluster::watch_registry::PauseReason;
use crate::tunnel::store::TunnelStore;
use crate::ui::picker_tunnel::TunnelChoice;
use crate::ui::resource_panel::ResourceEvent;
use crate::ui::status_bar::{StatusBarView, StatusItem};
use crate::util::shell::MainWindow;
use gpui_kit::{AppContext as _, TestAppContext, WeakEntity};
use kube::{Client, Config};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn init(cx: &mut TestAppContext) {
    // Some of the transitions below observe a real (never-connecting) `kube::Client`,
    // whose construction briefly enters the tokio runtime - the same seam
    // `k8s::cluster::session`'s and `ui/status_bar.rs`'s own tests use for this reason.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
}

fn test_client(cx: &mut TestAppContext) -> Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let _guard = handle.enter();
    Client::try_from(Config::new("http://127.0.0.1:0".parse().unwrap())).unwrap()
}

/// A scratch `tunnels.toml` path, distinct per call and per test process - never
/// this machine's real one.
fn temp_tunnels_path(label: &str) -> PathBuf {
    crate::util::test_paths::temp_path(&format!("status-capsule-{label}"))
}

/// A `WeakEntity<MainWindow>` to satisfy [`StatusBarView::for_window`].
/// Only the click-handling test below ever upgrades it; every other test just
/// needs a value of the right type to build a bar at all.
fn dummy_main_window(cx: &mut TestAppContext) -> WeakEntity<MainWindow> {
    let window = cx.add_window(MainWindow::test_picker_window);
    cx.update(|cx| {
        window
            .root(cx)
            .expect("the window this test just opened must still exist")
            .downgrade()
    })
}

/// Section 3.1: a single, unbound context's chip names it and shows no tunnel.
#[gpui_kit::test]
async fn chip_labels_show_context_name_and_no_tunnel_when_direct(cx: &mut TestAppContext) {
    init(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    let main_window = dummy_main_window(cx);

    let bar = cx.update(|cx| {
        cx.new(|cx| {
            StatusBarView::for_window(vec!["kind-dev".to_string()], main_window.clone(), cx)
        })
    });

    let chips: Vec<StatusItem> = cx.update(|cx| bar.read(cx).items(cx));
    assert_eq!(chips.len(), 1);
    assert_eq!(chips[0].context_name, "kind-dev");
    assert!(
        chips[0].active,
        "the window's only context is the active one"
    );
    assert_eq!(
        chips[0].tunnel, None,
        "no tunnel is bound, so the chip shows none"
    );
}

/// Section 3.1: a context bound to a tunnel shows that tunnel's name; one bound
/// to none does not - proven side by side so the label is provably per-context,
/// not a bar-wide flag.
#[gpui_kit::test]
async fn a_bound_context_shows_its_tunnels_name(cx: &mut TestAppContext) {
    init(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connecting);
    });
    let main_window = dummy_main_window(cx);

    let bar = cx.update(|cx| {
        cx.new(|cx| {
            let mut bar = StatusBarView::for_window(
                vec!["kind-dev".to_string(), "staging".to_string()],
                main_window.clone(),
                cx,
            );
            // Overriding the caches directly (rather than writing a real
            // `tunnels.toml`) isolates this test to `chips`' own mapping -
            // `a_tunnel_bound_after_the_bar_exists_appears_once_tunnels_revision_fires`
            // below is what proves the caches themselves load correctly from disk.
            bar.tunnel_choices = vec![TunnelChoice {
                id: "qa-bastion".to_string(),
                name: "QA".to_string(),
            }];
            bar.tunnel_bindings =
                BTreeMap::from([("staging".to_string(), "qa-bastion".to_string())]);
            bar
        })
    });

    let chips: Vec<StatusItem> = cx.update(|cx| bar.read(cx).items(cx));
    let kind_dev = chips
        .iter()
        .find(|chip| chip.context_name == "kind-dev")
        .expect("kind-dev has a chip");
    let staging = chips
        .iter()
        .find(|chip| chip.context_name == "staging")
        .expect("staging has a chip");
    assert_eq!(kind_dev.tunnel, None, "kind-dev has no binding");
    assert_eq!(
        staging.tunnel,
        Some("QA".to_string()),
        "staging's chip names its bound tunnel"
    );
}

/// Section 3.1's caching rule: a tunnel bound after the bar was built must not
/// appear until [`TunnelsRevision`](crate::ui::tunnels::TunnelsRevision) fires -
/// `chips` (the render path) must never read `tunnels.toml` itself. Asserting
/// the label is still absent right after the write, and present only after
/// `notify_tunnels_changed`, is what pins that ordering rather than just the
/// eventual result.
#[gpui_kit::test]
async fn a_tunnel_bound_after_the_bar_exists_appears_once_tunnels_revision_fires(
    cx: &mut TestAppContext,
) {
    init(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
    });
    let main_window = dummy_main_window(cx);
    let tunnels_path = temp_tunnels_path("bound-after-bar");

    let bar = cx.update(|cx| {
        cx.new(|cx| {
            let mut bar =
                StatusBarView::for_window(vec!["kind-dev".to_string()], main_window.clone(), cx);
            // Test-only override, the same pattern `ui/picker/tests.rs` uses on
            // `ClusterPicker`'s own `tunnels_path`: a fresh bar otherwise reads
            // this machine's real `tunnels.toml`, which this test must not touch.
            bar.tunnels_path = tunnels_path.clone();
            bar
        })
    });

    assert_eq!(
        cx.update(|cx| bar.read(cx).items(cx))[0].tunnel,
        None,
        "no tunnel bound yet"
    );

    let store = TunnelStore::new(tunnels_path.clone());
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
                ..Default::default()
            },
            None,
        )
        .unwrap();
    store.bind("kind-dev", "qa-bastion").unwrap();

    assert_eq!(
        cx.update(|cx| bar.read(cx).items(cx))[0].tunnel,
        None,
        "the bar must not read tunnels.toml on its own - only the revision observer does"
    );

    cx.update(crate::ui::tunnels::notify_tunnels_changed);
    cx.run_until_parked();

    assert_eq!(
        cx.update(|cx| bar.read(cx).items(cx))[0].tunnel,
        Some("QA".to_string()),
        "the newly bound tunnel appears once TunnelsRevision fires"
    );

    let _ = std::fs::remove_file(&tunnels_path);
}

/// Section 3.1: a chip's health dot matches `ClusterRegistry::health`'s own
/// severity - a context paused for reconnecting reads as a warning, the same
/// table `connection-status-bar`'s status bar uses.
#[gpui_kit::test]
async fn chip_health_reflects_cluster_registry_health(cx: &mut TestAppContext) {
    init(cx);
    let client = test_client(cx);
    cx.update(|cx| {
        // `apply_health_transition`'s `pause` is a no-op on a key nothing has
        // subscribed to (`WatchRegistry::pause`'s own untracked-key rule) - so a
        // pause needs a subscribed Pods watch to have anything to mark paused,
        // the same reason `ui/status_bar.rs`'s own tests subscribe before pausing.
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        );
        ClusterRegistry::subscribe_pods(cx, "kind-dev", client);
    });
    let main_window = dummy_main_window(cx);

    let bar = cx.update(|cx| {
        cx.new(|cx| {
            StatusBarView::for_window(vec!["kind-dev".to_string()], main_window.clone(), cx)
        })
    });

    assert_eq!(
        cx.update(|cx| bar.read(cx).items(cx))[0].severity,
        Severity::Muted,
        "a healthy connection's dot is muted"
    );

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        );
    });

    assert_eq!(
        cx.update(|cx| bar.read(cx).items(cx))[0].severity,
        Severity::Warning,
        "a context paused for reconnecting reads as a warning"
    );
}

/// Section 3: a chip click reaches [`MainWindow::set_active_context`] - the same
/// call the Resource panel's own cluster dropdown makes
/// (`ui/panel/resource.rs::ResourceEvent::SwitchContext`'s handler in
/// `util/shell.rs::enter_workspace`) - and the *real* bar `sync_context_children`
/// pushes state into (not a second, disconnected `StatusBarView` built only for
/// this test) reflects the new active chip. `util/shell/tests.rs`'s own
/// `set_active_context_switches_active_and_syncs_children` already proves
/// `MainWindow`'s side of this; what's new here is that the click handler this
/// module owns actually reaches it, and that `StatusBarView::chips` reports the
/// result correctly.
#[gpui_kit::test]
async fn chip_click_reaches_set_active_context_and_updates_the_real_bar(cx: &mut TestAppContext) {
    init(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connecting);
    });

    let window = cx.add_window(|window, cx| {
        MainWindow::test_workspace(
            vec!["kind-dev".to_string(), "staging".to_string()],
            window,
            cx,
        )
    });
    cx.run_until_parked();

    let bar = window
        .update(cx, |main_window, _window, _cx| {
            main_window
                .test_status_bar()
                .expect("a workspace-mode window has a status bar")
        })
        .unwrap();

    let active_chip = |cx: &mut TestAppContext| -> String {
        cx.update(|cx| {
            bar.read(cx)
                .items(cx)
                .into_iter()
                .find(|chip| chip.active)
                .expect("exactly one chip is active")
                .context_name
        })
    };
    assert_eq!(active_chip(cx), "kind-dev");

    // Not `window.update`: `activate` reaches back into
    // `MainWindow::set_active_context` through the same `WeakEntity<MainWindow>`
    // that is this window's own root view, and gpui rejects a second update of an
    // entity already mid-update. `update_window` borrows the window's `Window`/
    // `App` without reserving the root entity - the same access a real click's
    // dispatch has, since GPUI's own input handling never holds `MainWindow`
    // mid-update either.
    let any_window: gpui_kit::AnyWindowHandle = window.into();
    cx.update_window(any_window, |_root_view, window, app| {
        bar.update(app, |bar, cx| bar.activate("staging", window, cx));
    })
    .unwrap();

    assert_eq!(
        window
            .update(cx, |main_window, _window, _cx| {
                main_window.test_active_context_name()
            })
            .unwrap(),
        Some("staging".to_string()),
        "the click reached MainWindow::set_active_context"
    );
    assert_eq!(
        active_chip(cx),
        "staging",
        "sync_context_children pushed the new active index into the bar the window \
         actually renders"
    );
}

/// The Resource panel's cluster dropdown emits `SwitchContext` from inside its own
/// update, and `MainWindow` then re-syncs that same panel. Before the sync was
/// deferred, that re-entry panicked ("already being updated"); now the switch lands
/// and the bar follows.
#[gpui_kit::test]
async fn the_resource_dropdown_switches_the_active_context_without_reentering(
    cx: &mut TestAppContext,
) {
    init(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connecting);
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connecting);
    });
    let window = cx.add_window(|window, cx| {
        MainWindow::test_workspace(
            vec!["kind-dev".to_string(), "staging".to_string()],
            window,
            cx,
        )
    });
    cx.run_until_parked();

    let panel = window
        .update(cx, |main_window, _window, _cx| {
            main_window.test_resource_panel()
        })
        .unwrap()
        .expect("a workspace-mode window has a Resource panel");
    let any_window: gpui_kit::AnyWindowHandle = window.into();
    cx.update_window(any_window, |_root_view, _window, app| {
        panel.update(app, |_panel, cx| {
            cx.emit(ResourceEvent::SwitchContext("staging".to_string()));
        });
    })
    .unwrap();
    cx.run_until_parked();

    assert_eq!(
        window
            .update(cx, |main_window, _window, _cx| main_window
                .test_active_context_name())
            .unwrap(),
        Some("staging".to_string())
    );
}
