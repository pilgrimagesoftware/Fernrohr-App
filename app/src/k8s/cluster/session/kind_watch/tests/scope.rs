//! #446: a subscriber that needs one namespace shares an open cluster-wide watch when the
//! server allows it, and otherwise watches just that namespace - the fallback for a user
//! whose RBAC refuses cluster-wide lists.

use super::{deployments, fixture_client, secrets, session, wait_for};
use crate::k8s::cluster::session::{ClusterRegistry, KindKey, WatchKey};
use gpui_kit::TestAppContext;

fn refcount(cx: &mut TestAppContext, key: &KindKey) -> usize {
    cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .watchers
            .refcount(&WatchKey::Kind(key.clone()))
    })
}

fn namespaced(kind: &crate::k8s::cluster::discovery::DiscoveredKind) -> KindKey {
    KindKey {
        kind: kind.clone(),
        namespace: Some("staging".to_string()),
    }
}

/// With no cluster-wide watch open, the subscriber gets a watch of its namespace alone,
/// and its table holds only that namespace's objects.
#[gpui_kit::test]
async fn a_namespace_subscriber_watches_only_its_namespace(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = fixture_client(cx);
    session(cx, client.clone());
    let kind = deployments();

    let sub = cx.update(|cx| {
        ClusterRegistry::subscribe_kind_in(cx, "kind-dev", client, &kind, Some("staging"))
    });
    assert_eq!(sub.key, namespaced(&kind));
    wait_for(cx, &sub.table, |table| table.synced());
    let rows = cx.update(|cx| sub.table.read(cx).rows().len());
    assert_eq!(rows, 1, "the namespaced list, not the cluster-wide one");
    assert_eq!(
        refcount(cx, &KindKey::all(&kind)),
        0,
        "nothing cluster-wide"
    );

    cx.update(|cx| ClusterRegistry::unsubscribe_kind_in(cx, "kind-dev", &sub.key));
    assert_eq!(refcount(cx, &sub.key), 0);
}

/// An open cluster-wide watch serves the namespace subscriber: one table, no second stream.
#[gpui_kit::test]
async fn an_open_cluster_wide_watch_serves_a_namespace_subscriber(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = fixture_client(cx);
    session(cx, client.clone());
    let kind = deployments();

    let all =
        cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client.clone(), &kind));
    wait_for(cx, &all, |table| table.rows().len() == 2);
    let sub = cx.update(|cx| {
        ClusterRegistry::subscribe_kind_in(cx, "kind-dev", client, &kind, Some("staging"))
    });

    assert_eq!(sub.key, KindKey::all(&kind));
    assert_eq!(sub.table.entity_id(), all.entity_id(), "the same table");
    assert_eq!(refcount(cx, &KindKey::all(&kind)), 2);
    assert_eq!(refcount(cx, &namespaced(&kind)), 0, "no namespaced stream");

    cx.update(|cx| ClusterRegistry::unsubscribe_kind_in(cx, "kind-dev", &sub.key));
    assert_eq!(
        refcount(cx, &KindKey::all(&kind)),
        1,
        "the list still holds it"
    );
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
}

/// A cluster-wide list the server refuses isn't shared: the subscriber falls back to its
/// namespace, which the same user may list.
#[gpui_kit::test]
async fn a_refused_cluster_wide_list_falls_back_to_the_namespace(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = fixture_client(cx);
    session(cx, client.clone());
    let kind = secrets();

    let all =
        cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client.clone(), &kind));
    wait_for(cx, &all, |table| table.refused().is_some());
    let sub = cx.update(|cx| {
        ClusterRegistry::subscribe_kind_in(cx, "kind-dev", client, &kind, Some("staging"))
    });

    assert_eq!(sub.key, namespaced(&kind));
    wait_for(cx, &sub.table, |table| table.rows().len() == 1);
    assert!(cx.update(|cx| sub.table.read(cx).refused().is_none()));

    cx.update(|cx| ClusterRegistry::unsubscribe_kind_in(cx, "kind-dev", &sub.key));
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
}

/// A namespace subscriber's watch pauses and resumes with the rest of the session.
#[gpui_kit::test]
async fn a_namespaced_watch_pauses_and_resumes_with_the_session(cx: &mut TestAppContext) {
    use crate::k8s::cluster::health::HealthTransition;
    use crate::k8s::cluster::watch_registry::PauseReason;
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = fixture_client(cx);
    session(cx, client.clone());
    let kind = deployments();
    let sub = cx.update(|cx| {
        ClusterRegistry::subscribe_kind_in(cx, "kind-dev", client, &kind, Some("staging"))
    });

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });
    let running = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"].kinds[&sub.key]
                .task
                .is_some()
        })
    };
    assert!(!running(cx), "paused");
    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(cx, "kind-dev", HealthTransition::Resume)
    });
    assert!(running(cx), "restarted against its own namespace");
    cx.update(|cx| ClusterRegistry::unsubscribe_kind_in(cx, "kind-dev", &sub.key));
}

/// #457: a follow that shared a serving cluster-wide watch moves to its namespace once that
/// watch is refused, and releases its hold on the cluster-wide one.
#[gpui_kit::test]
async fn a_follow_moves_off_a_refused_cluster_wide_watch(cx: &mut TestAppContext) {
    use crate::k8s::cluster::session::NamespaceFollow;
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = fixture_client(cx);
    session(cx, client.clone());
    let kind = deployments();
    let all =
        cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client.clone(), &kind));
    wait_for(cx, &all, |table| table.rows().len() == 2);
    let mut follow =
        cx.update(|cx| NamespaceFollow::subscribe(cx, "kind-dev", client, &kind, Some("staging")));
    assert_eq!(
        follow.table().entity_id(),
        all.entity_id(),
        "shared at first"
    );
    assert!(
        !cx.update(|cx| follow.reroute_if_refused(cx)),
        "a serving watch keeps it"
    );

    cx.update(|cx| all.update(cx, |table, _| table.set_refused("forbidden".to_string())));
    assert!(cx.update(|cx| follow.reroute_if_refused(cx)), "moved");

    assert_eq!(refcount(cx, &namespaced(&kind)), 1);
    assert_eq!(
        refcount(cx, &KindKey::all(&kind)),
        1,
        "only the list's hold"
    );
    wait_for(cx, follow.table(), |table| table.rows().len() == 1);
    assert!(
        !cx.update(|cx| follow.reroute_if_refused(cx)),
        "a namespaced watch never moves"
    );

    cx.update(|cx| follow.release(cx));
    assert_eq!(refcount(cx, &namespaced(&kind)), 0);
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
}

/// A follow that needs every namespace has nothing narrower to move to.
#[gpui_kit::test]
async fn a_follow_of_every_namespace_stays_put(cx: &mut TestAppContext) {
    use crate::k8s::cluster::session::NamespaceFollow;
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = fixture_client(cx);
    session(cx, client.clone());
    let kind = secrets();
    let mut follow =
        cx.update(|cx| NamespaceFollow::subscribe(cx, "kind-dev", client, &kind, None));
    wait_for(cx, follow.table(), |table| table.refused().is_some());

    assert!(!cx.update(|cx| follow.reroute_if_refused(cx)));
    assert_eq!(refcount(cx, &KindKey::all(&kind)), 1);
    cx.update(|cx| follow.release(cx));
}
