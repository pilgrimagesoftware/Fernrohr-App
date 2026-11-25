//! #457: a reader of one namespace's pods shares the all-namespaces Pods watch while the
//! server allows it, and otherwise watches its namespace alone - including after the
//! all-namespaces watch it shared is refused.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::health::HealthTransition;
use crate::k8s::cluster::session::{ClusterRegistry, PodsFollow, WatchKey};
use crate::k8s::cluster::watch_registry::PauseReason;
use crate::k8s::resource::pods::PodsTable;
use crate::k8s::test_cluster::FakeCluster;
use gpui_kit::{Entity, TestAppContext};
use serde_json::json;

const CONTEXT: &str = "rbac-ctx";

/// A session over a fake cluster with one pod in each of `staging` and `prod`.
fn start(cx: &mut TestAppContext) -> (FakeCluster, kube::Client) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let (cluster, client) = FakeCluster::start(cx);
    for (name, namespace) in [("web-1", "staging"), ("api-1", "prod")] {
        cluster.apply(
            "/api/v1",
            "pods",
            json!({ "apiVersion": "v1", "kind": "Pod",
                "metadata": { "name": name, "namespace": namespace, "uid": format!("uid-{name}") } }),
        );
    }
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            CONTEXT,
            ConnectionState::Connected(client.clone()),
        );
    });
    (cluster, client)
}

fn refcount(cx: &mut TestAppContext, key: &WatchKey) -> usize {
    cx.update(|cx| ClusterRegistry::subscribers(cx, CONTEXT, key))
}

fn staging() -> WatchKey {
    WatchKey::PodsIn("staging".to_string())
}

/// Polls until `done` holds for `table`.
fn wait_for(cx: &mut TestAppContext, table: &Entity<PodsTable>, done: impl Fn(&PodsTable) -> bool) {
    for _ in 0..400 {
        cx.run_until_parked();
        if cx.update(|cx| done(table.read(cx))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let (pods, refused) = cx.update(|cx| {
        let table = table.read(cx);
        (table.pods().len(), table.refused().map(str::to_string))
    });
    panic!("never settled: {pods} pods, refused {refused:?}");
}

#[gpui_kit::test]
async fn with_no_open_watch_the_namespace_is_watched_alone(cx: &mut TestAppContext) {
    let (_cluster, client) = start(cx);

    let follow = cx.update(|cx| PodsFollow::subscribe(cx, CONTEXT, client, Some("staging")));
    wait_for(cx, follow.table(), |table| table.synced());

    assert_eq!(cx.update(|cx| follow.table().read(cx).pods().len()), 1);
    assert_eq!(refcount(cx, &staging()), 1);
    assert_eq!(refcount(cx, &WatchKey::Pods), 0, "nothing cluster-wide");
    cx.update(|cx| follow.release(cx));
    assert_eq!(refcount(cx, &staging()), 0);
}

#[gpui_kit::test]
async fn an_open_all_namespaces_watch_serves_the_reader(cx: &mut TestAppContext) {
    let (_cluster, client) = start(cx);
    let all = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, CONTEXT, client.clone()));
    wait_for(cx, &all, |table| table.pods().len() == 2);

    let follow = cx.update(|cx| PodsFollow::subscribe(cx, CONTEXT, client, Some("staging")));

    assert_eq!(
        follow.table().entity_id(),
        all.entity_id(),
        "the same table"
    );
    assert_eq!(refcount(cx, &WatchKey::Pods), 2);
    assert_eq!(refcount(cx, &staging()), 0);
    cx.update(|cx| follow.release(cx));
    assert_eq!(refcount(cx, &WatchKey::Pods), 1);
    cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, CONTEXT));
}

#[gpui_kit::test]
async fn a_refused_all_namespaces_watch_falls_back_to_the_namespace(cx: &mut TestAppContext) {
    let (cluster, client) = start(cx);
    cluster.allow_lists_only_in(&["staging"]);
    let all = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, CONTEXT, client.clone()));
    wait_for(cx, &all, |table| table.refused().is_some());

    let follow = cx.update(|cx| PodsFollow::subscribe(cx, CONTEXT, client, Some("staging")));
    wait_for(cx, follow.table(), |table| table.pods().len() == 1);

    assert!(cx.update(|cx| follow.table().read(cx).refused().is_none()));
    cx.update(|cx| follow.release(cx));
    cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, CONTEXT));
}

#[gpui_kit::test]
async fn a_reader_moves_off_an_all_namespaces_watch_once_refused(cx: &mut TestAppContext) {
    let (_cluster, client) = start(cx);
    let all = cx.update(|cx| ClusterRegistry::subscribe_pods(cx, CONTEXT, client.clone()));
    wait_for(cx, &all, |table| table.pods().len() == 2);
    let mut follow = cx.update(|cx| PodsFollow::subscribe(cx, CONTEXT, client, Some("staging")));
    assert!(
        !cx.update(|cx| follow.reroute_if_refused(cx)),
        "a serving watch keeps it"
    );

    cx.update(|cx| all.update(cx, |table, _| table.set_refused("forbidden".to_string())));
    assert!(cx.update(|cx| follow.reroute_if_refused(cx)), "moved");

    assert_eq!(
        refcount(cx, &WatchKey::Pods),
        1,
        "only the first reader's hold"
    );
    assert_eq!(refcount(cx, &staging()), 1);
    wait_for(cx, follow.table(), |table| table.pods().len() == 1);
    cx.update(|cx| follow.release(cx));
    assert_eq!(refcount(cx, &staging()), 0);
    cx.update(|cx| ClusterRegistry::unsubscribe_pods(cx, CONTEXT));
}

#[gpui_kit::test]
async fn a_namespace_watch_pauses_and_resumes_with_the_session(cx: &mut TestAppContext) {
    let (_cluster, client) = start(cx);
    let follow = cx.update(|cx| PodsFollow::subscribe(cx, CONTEXT, client, Some("staging")));
    let running = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions[CONTEXT].pods_in["staging"]
                .task
                .is_some()
        })
    };
    assert!(running(cx));

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            CONTEXT,
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });
    assert!(!running(cx), "paused");
    cx.update(|cx| ClusterRegistry::apply_health_transition(cx, CONTEXT, HealthTransition::Resume));
    assert!(running(cx), "restarted against its namespace");
    cx.update(|cx| follow.release(cx));
}
