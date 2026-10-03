// Named imports only: a `use super::*` here would re-glob gpui_kit test macro internals.
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::cluster::health::HealthTransition;
use crate::k8s::cluster::session::test_support::test_client;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::cluster::watch_registry::PauseReason;
use crate::k8s::resource::object_list::ObjectsTable;
use gpui_kit::{Entity, TestAppContext};
use kube::core::GroupVersionKind;
use serde_json::json;

fn deployments() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("apps", "v1", "Deployment"),
        plural: "deployments".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

fn secrets() -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "Secret"),
        plural: "secrets".into(),
        namespaced: true,
        verbs: Default::default(),
    }
}

fn refcount(cx: &mut TestAppContext, kind: &DiscoveredKind) -> usize {
    cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .watchers
            .refcount(&WatchKey::Kind(kind.clone()))
    })
}

fn session(cx: &mut TestAppContext, client: kube::Client) {
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(cx, "kind-dev", ConnectionState::Connected(client))
    });
}

/// Two Deployments in `staging` for the Deployment list; a 403 for the Secret list;
/// an empty, closed response for any watch request.
async fn serve() -> std::net::SocketAddr {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let line = String::from_utf8_lossy(&buffer[..read])
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let deployment = |uid: &str, name: &str| json!({ "metadata": { "uid": uid, "name": name, "namespace": "staging" } });
                let (status, body) = if line.contains("watch=true") {
                    ("200 OK", String::new())
                } else if line.contains("/apis/apps/v1/deployments") {
                    let list = json!({
                        "apiVersion": "apps/v1", "kind": "DeploymentList",
                        "metadata": { "resourceVersion": "1" },
                        "items": [deployment("d1", "web"), deployment("d2", "api")],
                    });
                    ("200 OK", list.to_string())
                } else if line.contains("/api/v1/secrets") {
                    let refused = json!({
                        "kind": "Status", "apiVersion": "v1", "status": "Failure",
                        "reason": "Forbidden", "code": 403,
                        "message": "secrets is forbidden: User \"dev\" cannot list resource \"secrets\"",
                    });
                    ("403 Forbidden", refused.to_string())
                } else {
                    let missing = json!({ "kind": "Status", "apiVersion": "v1",
                        "status": "Failure", "reason": "NotFound", "code": 404 });
                    ("404 Not Found", missing.to_string())
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    addr
}

fn fixture_client(cx: &mut TestAppContext) -> kube::Client {
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let addr = handle.block_on(serve());
    let _guard = handle.enter();
    kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap())).unwrap()
}

/// Polls until `done` holds for `table`, panicking with the table's state if it never does.
fn wait_for(
    cx: &mut TestAppContext,
    table: &Entity<ObjectsTable>,
    done: impl Fn(&ObjectsTable) -> bool,
) {
    for _ in 0..400 {
        cx.run_until_parked();
        if cx.update(|cx| done(table.read(cx))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let (rows, refused) = cx.update(|cx| {
        let table = table.read(cx);
        (table.rows().len(), table.refused().map(str::to_string))
    });
    panic!("never settled: {rows} rows, refused {refused:?}");
}

/// Spec: "Two lists of one kind share a watch" - one table, one refcounted stream,
/// stopped (and its table dropped) when the last panel unsubscribes.
#[gpui_kit::test]
async fn two_panels_of_one_kind_share_one_watch_and_table(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    session(cx, client.clone());
    let kind = deployments();

    let a = cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client.clone(), &kind));
    let b = cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client, &kind));
    assert_eq!(a.entity_id(), b.entity_id(), "both render from one table");
    assert_eq!(refcount(cx, &kind), 2);
    assert!(cx.update(|cx| ClusterRegistry::kind_watch_running(cx, "kind-dev", &kind)));

    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
    assert_eq!(refcount(cx, &kind), 1);
    assert!(cx.update(|cx| ClusterRegistry::kind_watch_running(cx, "kind-dev", &kind)));

    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
    assert_eq!(refcount(cx, &kind), 0);
    assert!(!cx.update(|cx| ClusterRegistry::kind_watch_running(cx, "kind-dev", &kind)));
}

/// A kind's watch and the Pods watch on one context count separately, and so do two
/// different kinds.
#[gpui_kit::test]
async fn kinds_and_pods_are_counted_independently(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    session(cx, client.clone());

    cx.update(|cx| ClusterRegistry::subscribe_pods(cx, "kind-dev", client.clone()));
    let a = cx.update(|cx| {
        ClusterRegistry::subscribe_kind(cx, "kind-dev", client.clone(), &deployments())
    });
    let b = cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client, &secrets()));
    assert_ne!(a.entity_id(), b.entity_id(), "each kind has its own table");

    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &deployments()));
    assert_eq!(refcount(cx, &deployments()), 0);
    assert_eq!(refcount(cx, &secrets()), 1);
    let pods = cx.update(|cx| {
        cx.global::<ClusterRegistry>().sessions["kind-dev"]
            .watchers
            .refcount(&WatchKey::Pods)
    });
    assert_eq!(pods, 1, "the Pods watch is untouched");
}

/// The watch lists the kind across namespaces and the table holds a row per object.
#[gpui_kit::test]
async fn a_kinds_objects_arrive_as_rows(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = fixture_client(cx);
    session(cx, client.clone());

    let table =
        cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client, &deployments()));
    wait_for(cx, &table, |table| table.rows().len() == 2);
    let mut names: Vec<String> = cx.update(|cx| {
        table
            .read(cx)
            .rows()
            .iter()
            .map(|row| row.name.clone())
            .collect()
    });
    names.sort_unstable();
    assert_eq!(names, ["api", "web"]);
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &deployments()));
}

/// Spec: "A kind the user cannot list" - a 403 on the initial list leaves the server's
/// refusal on the table, for the panel to show in place of rows, and stops the watch.
#[gpui_kit::test]
async fn a_forbidden_kind_carries_the_servers_refusal(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = fixture_client(cx);
    session(cx, client.clone());

    let table = cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client, &secrets()));
    wait_for(cx, &table, |table| table.refused().is_some());
    let refused = cx.update(|cx| table.read(cx).refused().map(str::to_string));
    assert_eq!(
        refused.as_deref(),
        Some("secrets is forbidden: User \"dev\" cannot list resource \"secrets\"")
    );
    assert!(cx.update(|cx| table.read(cx).rows().is_empty()));
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &secrets()));
}

/// A health pause stops every kind's watch without unsubscribing it; resuming restarts
/// it from the session's client.
#[gpui_kit::test]
async fn a_pause_stops_and_a_resume_restarts_a_kinds_watch(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let client = test_client(cx);
    session(cx, client.clone());
    let kind = deployments();
    cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client, &kind));

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });
    assert!(!cx.update(|cx| ClusterRegistry::kind_watch_running(cx, "kind-dev", &kind)));
    assert_eq!(refcount(cx, &kind), 1, "paused, not unsubscribed");

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(cx, "kind-dev", HealthTransition::Resume)
    });
    assert!(cx.update(|cx| ClusterRegistry::kind_watch_running(cx, "kind-dev", &kind)));
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
}

mod poll;
