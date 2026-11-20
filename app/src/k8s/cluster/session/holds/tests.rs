// Named imports only: a `use super::*` here would re-glob gpui_kit test macro internals.
use crate::config::tunnels::{TunnelAuth, TunnelConfig};
use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::cluster::session::test_support::test_client;
use crate::k8s::cluster::tunnel::{self, ForwardKey};
use crate::tunnel::store::TunnelStore;
use gpui_kit::{AppContext as _, TestAppContext, WindowId};

fn temp_path(label: &str) -> std::path::PathBuf {
    crate::util::test_paths::temp_path(&format!("session-hold-release-{label}"))
}

/// A one-context kubeconfig fixture, mirroring `k8s::cluster::tunnel`'s own
/// test fixtures - the seam that lets `tunnel::acquire_for_context` resolve a
/// target without touching this machine's real kubeconfig.
fn kubeconfig_fixture(context_name: &str, server: &str) -> std::path::PathBuf {
    let path = temp_path("kubeconfig");
    let yaml = format!(
        "apiVersion: v1\nkind: Config\nclusters:\n  - name: {context_name}\n    cluster:\n      server: {server}\ncontexts:\n  - name: {context_name}\n    context:\n      cluster: {context_name}\n      user: {context_name}\nusers:\n  - name: {context_name}\n    user: {{}}\n"
    );
    std::fs::write(&path, yaml).unwrap();
    path
}

fn sample_tunnel(name: &str) -> TunnelConfig {
    TunnelConfig {
        name: name.to_string(),
        bastion_user: "deploy".to_string(),
        bastion_host: "bastion.example.invalid".to_string(),
        bastion_port: 22,
        jump_hosts: Vec::new(),
        auth: TunnelAuth::default(),
        ..Default::default()
    }
}

#[gpui_kit::test]
async fn two_windows_share_one_session_and_the_first_release_keeps_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connected(client))
    });

    let window_a = WindowId::from(1);
    let window_b = WindowId::from(2);
    cx.update(|cx| ClusterRegistry::hold(cx, "kind-dev", window_a));
    cx.update(|cx| ClusterRegistry::hold(cx, "kind-dev", window_b));
    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "kind-dev")),
        2
    );

    cx.update(|cx| ClusterRegistry::release(cx, "kind-dev", window_a));
    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "kind-dev")),
        1,
        "the session, and window_b's watches, must survive one release"
    );
    assert!(cx.update(|cx| {
        cx.global::<ClusterRegistry>()
            .sessions
            .contains_key("kind-dev")
    }));

    cx.update(|cx| ClusterRegistry::release(cx, "kind-dev", window_b));
    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "kind-dev")),
        0,
        "the last release must drop the session"
    );
    assert!(cx.update(|cx| {
        !cx.global::<ClusterRegistry>()
            .sessions
            .contains_key("kind-dev")
    }));
}

/// `release` on a window that never held the context, or a context with no
/// session, is a no-op rather than a panic - closing a picker-mode window (no
/// context ever held) goes through the same `release_window` path.
#[gpui_kit::test]
async fn releasing_an_unknown_context_or_window_is_a_no_op(cx: &mut TestAppContext) {
    cx.update(|cx| {
        ClusterRegistry::release(cx, "never-held", WindowId::from(1));
        ClusterRegistry::release_window(cx, WindowId::from(1));
    });
    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "never-held")),
        0
    );
}

/// Tasks.md 1.3's session-level half: releasing every hold a window has, in
/// one call, only ever drops the sessions that window was the last holder of.
#[gpui_kit::test]
async fn release_window_drops_only_sessions_it_was_the_last_holder_of(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client_a = test_client(cx);
    let client_b = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "solo", ConnectionState::Connected(client_a));
        ClusterRegistry::insert_test_session(cx, "shared", ConnectionState::Connected(client_b));
    });

    let window_a = WindowId::from(10);
    let window_b = WindowId::from(20);
    cx.update(|cx| {
        ClusterRegistry::hold(cx, "solo", window_a);
        ClusterRegistry::hold(cx, "shared", window_a);
        ClusterRegistry::hold(cx, "shared", window_b);
    });

    cx.update(|cx| ClusterRegistry::release_window(cx, window_a));

    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "solo")),
        0,
        "window_a was solo's only holder"
    );
    assert_eq!(
        cx.update(|cx| ClusterRegistry::window_holder_count(cx, "shared")),
        1,
        "window_b still holds shared"
    );
}

/// Design.md's own risk note: a session's connection carries a real tunnel
/// forward, so the last release must be observable one layer down too - the
/// `ForwardRegistry` entry (`live_forward_keys`) disappearing, not just the
/// session map entry.
#[gpui_kit::test]
async fn last_release_drops_the_sessions_tunnel_forward(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);

    let tunnels_path = temp_path("tunnels.toml");
    let kubeconfig_path = kubeconfig_fixture("kind-dev", "https://10.0.0.1:6443");
    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create("test-bastion", sample_tunnel("test"), None)
        .unwrap();
    store.bind("kind-dev", "test-bastion").unwrap();

    let handle = cx
        .update(|cx| {
            tunnel::acquire_for_context(cx, &tunnels_path, Some(&kubeconfig_path), "kind-dev")
        })
        .unwrap()
        .expect("kind-dev is bound to test-bastion");
    let key = ForwardKey::Ssh {
        tunnel_id: "test-bastion".to_string(),
        host: "10.0.0.1".to_string(),
        port: 6443,
    };
    let client = test_client(cx);
    let connection = cx.new(|_| {
        ClusterConnection::test_with_state_and_forward(ConnectionState::Connected(client), handle)
    });
    cx.update(|cx| {
        ClusterRegistry::insert_test_session_with_connection(cx, "kind-dev", connection)
    });

    let window_a = WindowId::from(1);
    let window_b = WindowId::from(2);
    cx.update(|cx| {
        ClusterRegistry::hold(cx, "kind-dev", window_a);
        ClusterRegistry::hold(cx, "kind-dev", window_b);
    });

    let live = cx.update(tunnel::live_forward_keys);
    assert!(live.borrow().contains(&key));

    cx.update(|cx| ClusterRegistry::release(cx, "kind-dev", window_a));
    cx.run_until_parked();
    assert!(
        live.borrow().contains(&key),
        "one window still holds kind-dev"
    );

    cx.update(|cx| ClusterRegistry::release(cx, "kind-dev", window_b));
    cx.run_until_parked();
    assert!(
        !live.borrow().contains(&key),
        "the last release must drop the tunnel forward too"
    );

    let _ = std::fs::remove_file(&tunnels_path);
    let _ = std::fs::remove_file(&kubeconfig_path);
}

/// `mcp-connections` "Window closes while an agent holds the context": a window
/// and an agent share one session; closing the window keeps it for the agent, who
/// is then its only holder and isn't counted as a window.
#[gpui_kit::test]
async fn a_window_closing_keeps_a_context_an_agent_holds(cx: &mut TestAppContext) {
    use crate::k8s::cluster::session::{AgentId, Holder};
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connected(client))
    });
    let window = WindowId::from(1);
    let agent = AgentId::next();

    cx.update(|cx| ClusterRegistry::hold(cx, "kind-dev", agent));
    cx.update(|cx| ClusterRegistry::hold(cx, "kind-dev", window));
    cx.update(|cx| {
        assert_eq!(
            ClusterRegistry::holders(cx, "kind-dev"),
            vec![Holder::Window(window), Holder::Agent(agent)],
            "windows first, then agents"
        );
        assert_eq!(ClusterRegistry::window_holder_count(cx, "kind-dev"), 1);
        assert_eq!(
            ClusterRegistry::connected_by(cx, "kind-dev"),
            Some(Holder::Agent(agent)),
            "the agent held it first"
        );
    });

    cx.update(|cx| ClusterRegistry::release_window(cx, window));
    cx.update(|cx| {
        assert!(
            ClusterRegistry::existing_connection(cx, "kind-dev").is_some(),
            "the agent's hold keeps the connection"
        );
        assert_eq!(
            ClusterRegistry::holders(cx, "kind-dev"),
            vec![Holder::Agent(agent)]
        );
        assert_eq!(ClusterRegistry::window_holder_count(cx, "kind-dev"), 0);
    });
}

/// `mcp-connections` "Agent's session ends last": releasing the last holder,
/// an agent, drops the session as a window's last release does; a context a
/// window also holds stays.
#[gpui_kit::test]
async fn an_agent_releasing_last_drops_only_the_sessions_it_alone_held(cx: &mut TestAppContext) {
    use crate::k8s::cluster::session::AgentId;
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        );
        ClusterRegistry::insert_test_session(cx, "staging", ConnectionState::Connected(client));
    });
    let agent = AgentId::next();
    let window = WindowId::from(7);
    cx.update(|cx| {
        ClusterRegistry::hold(cx, "kind-dev", agent);
        ClusterRegistry::hold(cx, "staging", agent);
        ClusterRegistry::hold(cx, "staging", window);
    });

    cx.update(|cx| ClusterRegistry::release_agent(cx, agent));
    cx.update(|cx| {
        assert!(
            ClusterRegistry::existing_connection(cx, "kind-dev").is_none(),
            "the agent was kind-dev's last holder"
        );
        assert!(
            ClusterRegistry::existing_connection(cx, "staging").is_some(),
            "the window still holds staging"
        );
    });
}

/// Agent handles are never reused, so a hold can't pass from an ended agent to a
/// new one.
#[test]
fn agent_ids_are_never_reused() {
    use crate::k8s::cluster::session::AgentId;
    let ids: std::collections::HashSet<AgentId> = (0..1000).map(|_| AgentId::next()).collect();
    assert_eq!(ids.len(), 1000);
}

/// `mcp-connections` "Client exits": an ended agent's hold lingers - the
/// connection stays open, held by nobody who can act for it - and is released
/// when the grace period ends. Releasing the agent again changes nothing: the
/// lingering hold isn't its any more.
#[gpui_kit::test]
async fn an_ended_agents_hold_lingers_for_the_grace_period_then_goes(cx: &mut TestAppContext) {
    use crate::k8s::cluster::session::{AgentId, Holder};
    use std::time::Duration;
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connected(client))
    });
    let agent = AgentId::next();
    cx.update(|cx| ClusterRegistry::hold(cx, "kind-dev", agent));

    let grace = Duration::from_secs(30);
    cx.update(|cx| ClusterRegistry::linger_agent(cx, agent, grace));
    cx.update(|cx| {
        let holders = ClusterRegistry::holders(cx, "kind-dev");
        assert!(
            matches!(holders.as_slice(), [Holder::Lingering(_)]),
            "the agent's hold became a lingering one: {holders:?}"
        );
        ClusterRegistry::release_agent(cx, agent);
        assert!(
            ClusterRegistry::existing_connection(cx, "kind-dev").is_some(),
            "the ended agent can't release, or claim, the lingering hold"
        );
    });

    cx.executor().advance_clock(grace - Duration::from_secs(1));
    cx.run_until_parked();
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "kind-dev").is_some()),
        "still open inside the grace period"
    );
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(
        cx.update(|cx| ClusterRegistry::existing_connection(cx, "kind-dev").is_none()),
        "released when the grace period ends"
    );
}

/// A window holding the context too keeps it after the lingering hold goes.
#[gpui_kit::test]
async fn a_lingering_hold_ending_leaves_a_windows_hold(cx: &mut TestAppContext) {
    use crate::k8s::cluster::session::{AgentId, Holder};
    use std::time::Duration;
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connected(client))
    });
    let agent = AgentId::next();
    let window = WindowId::from(3);
    cx.update(|cx| {
        ClusterRegistry::hold(cx, "kind-dev", agent);
        ClusterRegistry::hold(cx, "kind-dev", window);
        ClusterRegistry::linger_agent(cx, agent, Duration::from_secs(30));
    });
    cx.executor().advance_clock(Duration::from_secs(31));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| ClusterRegistry::holders(cx, "kind-dev")),
        vec![Holder::Window(window)]
    );
}
