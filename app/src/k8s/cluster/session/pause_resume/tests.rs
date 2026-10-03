// Named imports only: a `use super::*` here would re-glob gpui_kit test macro internals.
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::context_health::ContextHealth;
use crate::k8s::cluster::health::HealthTransition;
use crate::k8s::cluster::session::test_support::test_client;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::cluster::watch_registry::PauseReason;
use gpui_kit::TestAppContext;

/// `connection-status-bar` task 1.1: `health` reads `Connected` for a session whose
/// connection hasn't failed, isn't waiting for a tunnel, and has nothing paused -
/// which includes `Connecting`, since the spec has no distinct row for it (see
/// `context_health::ContextHealth`'s own doc comment).
#[gpui_kit::test]
async fn health_is_connected_when_nothing_is_wrong(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

    assert_eq!(
        cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")),
        ContextHealth::Connected
    );
}

#[gpui_kit::test]
async fn health_reads_waiting_for_tunnel_from_the_connection(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

    let connection = cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .connection
            .clone()
    });
    cx.update(|cx| {
        connection.update(cx, |connection, cx| {
            connection.state = ConnectionState::WaitingForTunnel;
            cx.notify();
        })
    });

    assert!(matches!(
        cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")),
        ContextHealth::WaitingForTunnel { .. }
    ));
}

#[gpui_kit::test]
async fn health_reads_failed_from_the_connection(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

    let connection = cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .connection
            .clone()
    });
    cx.update(|cx| {
        connection.update(cx, |connection, cx| {
            connection.state = ConnectionState::Failed("boom".to_string());
            cx.notify();
        })
    });

    match cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")) {
        ContextHealth::Failed { reason, .. } => assert_eq!(reason, "boom"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[gpui_kit::test]
async fn health_reads_paused_on_the_pods_key(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });

    match cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")) {
        ContextHealth::Paused { reason, .. } => assert_eq!(reason, PauseReason::Reconnecting),
        other => panic!("expected Paused, got {other:?}"),
    }
}

/// `first_paused` (`watch_registry.rs`) is what makes this work for a kind other than
/// `"pods"` - `health` never has a hardcoded key of its own.
#[gpui_kit::test]
async fn health_reads_paused_on_a_kind_other_than_pods(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

    cx.update(|cx| {
        let session = cx
            .global_mut::<ClusterRegistry>()
            .sessions
            .get_mut("kind-dev")
            .unwrap();
        let events = WatchKey::Kind(crate::k8s::cluster::discovery::DiscoveredKind {
            gvk: kube::core::GroupVersionKind::gvk("", "v1", "Event"),
            plural: "events".into(),
            namespaced: true,
            verbs: Default::default(),
        });
        session.watchers.subscribe(events.clone());
        session
            .watchers
            .pause(&events, PauseReason::CredentialRefresh);
    });

    match cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")) {
        ContextHealth::Paused { reason, .. } => {
            assert_eq!(reason, PauseReason::CredentialRefresh)
        }
        other => panic!("expected Paused, got {other:?}"),
    }
}

#[gpui_kit::test]
async fn health_prefers_failed_over_paused(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });
    let connection = cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .connection
            .clone()
    });
    cx.update(|cx| {
        connection.update(cx, |connection, cx| {
            connection.state = ConnectionState::Failed("boom".to_string());
            cx.notify();
        })
    });

    assert!(matches!(
        cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")),
        ContextHealth::Failed { .. }
    ));
}

#[gpui_kit::test]
async fn health_prefers_paused_over_waiting_for_tunnel(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

    let connection = cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .connection
            .clone()
    });
    cx.update(|cx| {
        connection.update(cx, |connection, cx| {
            connection.state = ConnectionState::WaitingForTunnel;
            cx.notify();
        })
    });
    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });

    assert!(matches!(
        cx.update(|cx| ClusterRegistry::health(cx, "kind-dev")),
        ContextHealth::Paused { .. }
    ));
}

/// `connection-status-bar` task 1.3's audit: every pause/resume write already goes
/// through `cx.global_mut::<Self>()`, which pushes `Effect::NotifyGlobalObservers`
/// unconditionally on every call (see `gpui`'s `App::global_mut`) - so
/// `observe_global::<ClusterRegistry>` already sees every edge with no extra
/// plumbing. Proven here against the real production entry point,
/// `apply_health_transition`, rather than trusted from reading the source.
///
/// This does not cover `ConnectionState` transitions (`WaitingForTunnel` -> `Failed`,
/// for instance): those notify their own `ClusterConnection` entity, not this global,
/// so `ui/status_bar.rs` additionally observes each shown context's connection
/// entity directly - the "smaller and correct" fix design.md decision 4 allows for,
/// since routing connection-state writes through the registry global as well would
/// make `connection.rs` depend on `session.rs` for no benefit.
#[gpui_kit::test]
async fn pause_and_resume_each_notify_registry_observers(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;

    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));

    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _subscription = cx.update(|cx| {
        cx.observe_global::<ClusterRegistry>(move |_cx| observed.set(observed.get() + 1))
    });

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });
    assert_eq!(
        notifications.get(),
        1,
        "pausing must notify registry observers"
    );

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(cx, "kind-dev", HealthTransition::Resume)
    });
    assert_eq!(
        notifications.get(),
        2,
        "resuming must notify registry observers"
    );
}
