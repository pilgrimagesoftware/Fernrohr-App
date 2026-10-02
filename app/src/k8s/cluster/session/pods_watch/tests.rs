// Named imports only: a `use super::*` here would re-glob gpui_kit test macro internals.
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::health::HealthTransition;
use crate::k8s::cluster::session::test_support::test_client;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::cluster::watch_registry::PauseReason;
use gpui_kit::TestAppContext;

#[gpui_kit::test]
async fn two_subscribers_share_one_pods_watch_and_table(cx: &mut TestAppContext) {
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
    let table_a = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client.clone()));
    let table_b = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client));
    assert_eq!(
        table_a.entity_id(),
        table_b.entity_id(),
        "two subscribers should render from the same table"
    );

    // First unsubscribe (2-to-1) must not tear the watch down; only the
    // second (1-to-0) should.
    cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "kind-dev"));
    assert_eq!(
        cx.update(|cx| cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .watchers
            .refcount(&WatchKey::Pods)),
        1
    );
    cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "kind-dev"));
    assert_eq!(
        cx.update(|cx| cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .watchers
            .refcount(&WatchKey::Pods)),
        0
    );
}

/// Two different context names get independent sessions: independent tables and
/// independent watch refcounts, the point of rekeying `ClusterSession` by context.
#[gpui_kit::test]
async fn different_contexts_get_independent_sessions(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);

    let client_a = test_client(cx);
    let client_b = test_client(cx);
    let table_a = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "dev", client_a));
    let table_b = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "staging", client_b));

    assert_ne!(
        table_a.entity_id(),
        table_b.entity_id(),
        "different contexts must not share a table"
    );
    assert_eq!(
        cx.update(|cx| cx.global::<ClusterRegistry>().sessions["dev"]
            .watchers
            .refcount(&WatchKey::Pods)),
        1
    );
    assert_eq!(
        cx.update(|cx| cx.global::<ClusterRegistry>().sessions["staging"]
            .watchers
            .refcount(&WatchKey::Pods)),
        1
    );

    cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "dev"));
    assert_eq!(
        cx.update(|cx| cx.global::<ClusterRegistry>().sessions["dev"]
            .watchers
            .refcount(&WatchKey::Pods)),
        0
    );
    assert_eq!(
        cx.update(|cx| cx.global::<ClusterRegistry>().sessions["staging"]
            .watchers
            .refcount(&WatchKey::Pods)),
        1,
        "unsubscribing one context must not affect another"
    );
}

/// Section 7.3's two pieces, tested independently and hermetically rather than through
/// `handle_pods_unauthorized` itself: that function calls the real `kube::Config::infer`
/// to refresh a credential, which would read this machine's actual kubeconfig and
/// attempt a real network probe if driven directly in a test - not something a unit
/// test should do. `apply_health_transition` (the pause step, section 7.2) and
/// `refresh_client` (the resume-with-a-new-client step) are exactly what
/// `handle_pods_unauthorized` composes; each is exercised here on its own, the same
/// seam section 6.2's `connect_and_probe` tests already drove one layer down.
#[gpui_kit::test]
async fn unauthorized_pause_stops_the_watch_and_refresh_resumes_it(cx: &mut TestAppContext) {
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
    assert!(cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .pods_watch
            .is_some()
    }));

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::CredentialRefresh),
        )
    });
    assert!(cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .watchers
            .is_paused(&WatchKey::Pods)
    }));
    assert!(cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .pods_watch
            .is_none()
    }));

    let refreshed_client = test_client(cx);
    cx.update(|cx| ClusterRegistry::refresh_client(cx, "kind-dev", refreshed_client));
    assert!(!cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .watchers
            .is_paused(&WatchKey::Pods)
    }));
    assert!(cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .pods_watch
            .is_some()
    }));
}

/// `refresh_client` arriving after every panel already unsubscribed (the watch
/// entry gone entirely, section 7.1's teardown-drops-the-flag behavior) must not
/// resurrect a watch nobody is subscribed to.
#[gpui_kit::test]
async fn refresh_after_every_panel_unsubscribed_does_not_restart_the_watch(
    cx: &mut TestAppContext,
) {
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
            HealthTransition::Pause(PauseReason::CredentialRefresh),
        )
    });
    cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, "kind-dev"));

    let refreshed_client = test_client(cx);
    cx.update(|cx| ClusterRegistry::refresh_client(cx, "kind-dev", refreshed_client));

    assert!(cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .pods_watch
            .is_none()
    }));
    assert_eq!(
        cx.update(|cx| cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .watchers
            .refcount(&WatchKey::Pods)),
        0
    );
}
