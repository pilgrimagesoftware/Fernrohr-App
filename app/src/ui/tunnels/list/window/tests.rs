// NAMED imports only (no `use super::*;`): a glob of `gpui_kit::*` next to
// `#[gpui_kit::test]` shadows the builtin `#[test]` and blows the macro-expansion budget.
use crate::config::tunnels::{TunnelAuth, TunnelConfig};
use crate::k8s::cluster::tunnel::ForwardKey;
use crate::tunnel::store::TunnelStore;
use crate::ui::tunnels::list::TunnelsWindow;
use gpui_kit::TestAppContext;
use std::path::PathBuf;

fn temp_tunnels_path() -> PathBuf {
    crate::util::test_paths::temp_path("tunnels-window")
}

fn missing_kubeconfig_path() -> PathBuf {
    std::env::temp_dir().join("fernrohr-tunnels-window-test-no-such-kubeconfig.yaml")
}

fn sample_tunnel(name: &str) -> TunnelConfig {
    TunnelConfig {
        name: name.to_string(),
        bastion_user: "ops".into(),
        bastion_host: "bastion.example.com".into(),
        bastion_port: 22,
        jump_hosts: Vec::new(),
        auth: TunnelAuth::default(),
        ..Default::default()
    }
}

/// Tasks.md 4.1: opening the Tunnels window twice must not create a second
/// window - the second call focuses the one already open. Exercises
/// `TunnelsWindow` construction and `is_running` directly (the single-instance
/// `open_or_focus`/`WindowHandle` bookkeeping is exercised structurally by its own
/// short-circuit on `activate_window`, which needs a real platform window this
/// harness does not create) - see `a_running_state_follows_acquire_and_release`
/// below for the behavior `open_or_focus` exists to show.
#[gpui_kit::test]
async fn a_second_construction_over_the_same_files_reads_the_same_state(cx: &mut TestAppContext) {
    // `TunnelsWindow::new` always starts `watch_running_state`'s real tokio task
    // (via `spawn_stream`), even with no forward ever acquired - the same seam
    // `cluster::session`'s and `cluster::connection`'s own tests allow-park for.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let tunnels_path = temp_tunnels_path();
    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("qa-bastion", sample_tunnel("QA"), None)
        .unwrap();

    let kubeconfig_path = missing_kubeconfig_path();
    let window = cx.add_window({
        let tunnels_path = tunnels_path.clone();
        let kubeconfig_path = kubeconfig_path.clone();
        move |window, cx| TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
    });
    let second = cx.add_window({
        let tunnels_path = tunnels_path.clone();
        let kubeconfig_path = kubeconfig_path.clone();
        move |window, cx| TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
    });

    window
        .update(cx, |this, _window, _cx| {
            assert_eq!(this.tunnels.len(), 1);
            assert_eq!(this.tunnels[0].0, "qa-bastion");
        })
        .unwrap();
    second
        .update(cx, |this, _window, _cx| {
            assert_eq!(this.tunnels.len(), 1);
        })
        .unwrap();

    let _ = std::fs::remove_file(&tunnels_path);
}

/// Tasks.md 4.1: running state follows a fake forward's acquire and release -
/// simulated the same way `k8s::cluster::tunnel`'s own `drive_live_keys` test
/// simulates one: a hand-driven `BTreeSet<ForwardKey>`, since `TunnelForwards`'s
/// registry is private and only ever populated through a real `ssh` acquire.
#[gpui_kit::test]
async fn a_running_state_follows_acquire_and_release(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let tunnels_path = temp_tunnels_path();
    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("qa-bastion", sample_tunnel("QA"), None)
        .unwrap();

    let kubeconfig_path = missing_kubeconfig_path();
    let window = cx.add_window({
        let tunnels_path = tunnels_path.clone();
        move |window, cx| TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
    });

    window
        .update(cx, |this, _window, _cx| {
            assert!(!this.is_running("qa-bastion"));
            this.running = std::collections::BTreeSet::from([ForwardKey::Ssh {
                tunnel_id: "qa-bastion".to_string(),
                host: "10.0.0.1".to_string(),
                port: 6443,
            }]);
            assert!(this.is_running("qa-bastion"));
            this.running.clear();
            assert!(!this.is_running("qa-bastion"));
        })
        .unwrap();

    let _ = std::fs::remove_file(&tunnels_path);
}

/// Tasks.md 4.1: Remove deletes a stale binding, and it drops out of the list on
/// the very next refresh.
#[gpui_kit::test]
async fn remove_deletes_a_stale_binding(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let tunnels_path = temp_tunnels_path();
    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("qa-bastion", sample_tunnel("QA"), None)
        .unwrap();
    store.bind("renamed-away", "qa-bastion").unwrap();

    // An empty (but present) kubeconfig fixture: `renamed-away` is bound but
    // absent from it, so it is genuinely stale rather than "unknown because the
    // kubeconfig couldn't be read".
    let kubeconfig_path = std::env::temp_dir().join(format!(
        "fernrohr-tunnels-window-test-empty-kubeconfig-{}.yaml",
        std::process::id()
    ));
    std::fs::write(
        &kubeconfig_path,
        "apiVersion: v1\nkind: Config\nclusters: []\ncontexts: []\nusers: []\n",
    )
    .unwrap();

    let window = cx.add_window({
        let tunnels_path = tunnels_path.clone();
        let kubeconfig_path = kubeconfig_path.clone();
        move |window, cx| TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
    });

    window
        .update(cx, |this, _window, _cx| {
            assert_eq!(
                this.stale,
                vec![("renamed-away".to_string(), "qa-bastion".to_string())]
            );
        })
        .unwrap();

    window
        .update(cx, |this, _window, cx| {
            this.remove_stale("renamed-away".to_string(), cx);
        })
        .unwrap();

    window
        .update(cx, |this, _window, _cx| {
            assert!(this.stale.is_empty());
        })
        .unwrap();
    assert!(store.bindings().is_empty());

    let _ = std::fs::remove_file(&tunnels_path);
    let _ = std::fs::remove_file(&kubeconfig_path);
}

/// An unreadable kubeconfig leaves the context list unknown, so no binding is
/// offered for removal - otherwise every good binding would look stale.
#[gpui_kit::test]
async fn an_unreadable_kubeconfig_flags_nothing_stale(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let tunnels_path = temp_tunnels_path();
    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("qa-bastion", sample_tunnel("QA"), None)
        .unwrap();
    store.bind("southbay", "qa-bastion").unwrap();
    let missing = std::env::temp_dir().join(format!(
        "fernrohr-tunnels-window-test-missing-kubeconfig-{}.yaml",
        std::process::id()
    ));

    let window = cx.add_window({
        let tunnels_path = tunnels_path.clone();
        move |window, cx| TunnelsWindow::new(tunnels_path, Some(missing), window, cx)
    });
    window
        .update(cx, |this, _window, _cx| assert!(this.stale.is_empty()))
        .unwrap();

    let _ = std::fs::remove_file(&tunnels_path);
}

/// `visual-refresh-typography-spacing` 3.2, dialogs and windows: the Tunnels
/// window's content sits at least the panel inset from its edges - the New
/// Tunnel button, at the header's top right, from the top and right. At 150%
/// text, so the inset is the scaled token, not the 16px literal it replaced.
#[gpui_kit::test]
async fn the_tunnels_windows_content_is_inset_from_its_edges(cx: &mut TestAppContext) {
    use crate::ui::space::{TextScale, spacing};
    use gpui_kit::test::TestWindowExt as _;

    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
        TextScale::new(1.5).expect("a valid scale").set(cx);
    });
    let tunnels_path = temp_tunnels_path();
    let kubeconfig_path = missing_kubeconfig_path();
    let window = cx.add_window(move |window, cx| {
        TunnelsWindow::new(tunnels_path, Some(kubeconfig_path), window, cx)
    });
    let any: gpui_kit::AnyWindowHandle = window.into();
    let (inset, width, button) = any
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let button = window
                .try_find(gpui_kit::ElementId::from(gpui_kit::SharedString::from(
                    "tunnels-new",
                )))
                .expect("the New Tunnel button is drawn")
                .bounds();
            (
                spacing(cx).panel_inset,
                window.viewport_size().width,
                button,
            )
        })
        .unwrap();
    assert!(
        button.top() >= inset,
        "the button is {:?} from the top, under {inset:?}",
        button.top()
    );
    assert!(
        width - button.right() >= inset,
        "the button is {:?} from the right, under {inset:?}",
        width - button.right()
    );
}

/// `k9s-remaining-keybindings` 4.3, end to end over a fake cluster's Running pod:
/// a forward started from a row is listed in the Tunnels window in its current
/// state, and its Stop releases it through the registry - gone from the list
/// and from the live set.
#[gpui_kit::test]
async fn a_row_started_forward_is_listed_and_stop_releases_it(cx: &mut TestAppContext) {
    use crate::forward::managed::ForwardState;
    use crate::k8s::cluster::port_forwards::{PortForwardRequest, PortForwards};
    use crate::ui::tunnels::list::port_forwards::stop_button_id;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{Modifiers, VisualTestContext};

    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let (cluster, client) = crate::k8s::test_cluster::FakeCluster::start(cx);
    cluster.apply(
        "/api/v1",
        "pods",
        serde_json::json!({ "apiVersion": "v1", "kind": "Pod",
            "metadata": { "name": "web-1", "namespace": "shop", "uid": "u1" },
            "spec": { "containers": [{ "name": "app", "image": "nginx" }] },
            "status": { "phase": "Running" } }),
    );
    let request = PortForwardRequest {
        context_name: "demo".into(),
        namespace: "shop".into(),
        pod: "web-1".into(),
        remote_port: 18_084,
    };
    let forwards = cx.update(PortForwards::entity);
    forwards
        .update(cx, |forwards, cx| {
            forwards.start(request.clone(), client, cx)
        })
        .expect("started");

    let window = cx.add_window(|window, cx| {
        TunnelsWindow::new(
            temp_tunnels_path(),
            Some(missing_kubeconfig_path()),
            window,
            cx,
        )
    });
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    for _ in 0..400 {
        vcx.run_until_parked();
        let up = forwards.read_with(&vcx, |forwards, _| {
            forwards.list().first().map(|(_, _, state)| *state) == Some(ForwardState::Up)
        });
        if up {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let stop_id: &'static str = Box::leak(stop_button_id(0).to_string().into_boxed_str());
    let stop = vcx.update(|window, cx| {
        window.render_frame(cx);
        window
            .try_find(stop_id)
            .expect("the forward is listed, with its Stop")
            .bounds()
    });

    vcx.simulate_click(stop.center(), Modifiers::none());
    vcx.run_until_parked();

    forwards.read_with(&vcx, |forwards, _| {
        assert!(forwards.list().is_empty(), "gone from the list");
        assert!(forwards.live_requests().is_empty(), "and released");
    });
    let listed = vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(stop_id).is_some()
    });
    assert!(!listed, "and from the window");
}
