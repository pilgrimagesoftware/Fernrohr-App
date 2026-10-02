//! `events-browser` 1.1: the shared typed Event watch. A mock API server serves
//! one legacy-field event and one `events.k8s.io/v1`-only event, then its first
//! watch adds, modifies and deletes; later lists reflect the same state.

// Named imports only: a `use super::*` here would re-glob gpui_kit test macro internals.
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::test_support::test_client;
use crate::k8s::cluster::session::{ClusterRegistry, WatchKey};
use crate::k8s::resource::events_browser::EventsTable;
use gpui_kit::{Entity, TestAppContext};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A legacy event: `lastTimestamp`, `count` and `source`.
fn legacy(count: i32) -> Value {
    json!({
        "metadata": { "uid": "e1", "name": "web.1", "namespace": "payments" },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "payments", "name": "web-1" },
        "type": "Warning", "reason": "BackOff", "message": "Back-off restarting failed container",
        "count": count, "lastTimestamp": "2026-10-02T10:00:00Z",
        "source": { "component": "kubelet", "host": "node-a" },
    })
}

/// An event written through `events.k8s.io/v1`: no legacy timestamps or count -
/// `eventTime`, `series` and the reporting fields instead.
fn v1_only() -> Value {
    json!({
        "metadata": { "uid": "e2", "name": "web.2", "namespace": "payments" },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "payments", "name": "web-2" },
        "type": "Normal", "reason": "Scheduled", "message": "Successfully assigned payments/web-2",
        "eventTime": "2026-10-02T09:00:00.000000Z",
        "series": { "count": 3, "lastObservedTime": "2026-10-02T09:30:00.000000Z" },
        "reportingComponent": "default-scheduler", "reportingInstance": "scheduler-0",
    })
}

fn added() -> Value {
    json!({
        "metadata": { "uid": "e3", "name": "api.1", "namespace": "staging" },
        "involvedObject": { "kind": "Deployment", "apiVersion": "apps/v1", "namespace": "staging", "name": "api" },
        "type": "Normal", "reason": "ScalingReplicaSet", "message": "Scaled up replica set api-7d9f to 3",
        "count": 1, "lastTimestamp": "2026-10-02T11:00:00Z",
        "source": { "component": "deployment-controller" },
    })
}

/// Serves the event list, and on the first watch a stream that adds `e3`,
/// bumps `e1`'s count and deletes `e2`; every later list shows that result.
async fn serve() -> std::net::SocketAddr {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let watched = Arc::new(AtomicBool::new(false));
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let watched = watched.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let line = String::from_utf8_lossy(&buffer[..read])
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let body = if line.contains("watch=true") {
                    if watched.swap(true, Ordering::SeqCst) {
                        String::new()
                    } else {
                        [
                            json!({ "type": "ADDED", "object": added() }),
                            json!({ "type": "MODIFIED", "object": legacy(5) }),
                            json!({ "type": "DELETED", "object": v1_only() }),
                        ]
                        .iter()
                        .map(|event| format!("{event}\n"))
                        .collect()
                    }
                } else {
                    let items = if watched.load(Ordering::SeqCst) {
                        vec![legacy(5), added()]
                    } else {
                        vec![legacy(1), v1_only()]
                    };
                    json!({ "apiVersion": "v1", "kind": "EventList", "metadata": { "resourceVersion": "1" }, "items": items })
                        .to_string()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    addr
}

fn connected(cx: &mut TestAppContext) -> kube::Client {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let addr = handle.block_on(serve());
    let client = {
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
            .unwrap()
    };
    cx.update(|cx| {
        ClusterRegistry::insert_test_session(
            cx,
            "kind-dev",
            ConnectionState::Connected(client.clone()),
        )
    });
    client
}

fn wait_for(
    cx: &mut TestAppContext,
    table: &Entity<EventsTable>,
    done: impl Fn(&EventsTable) -> bool,
) {
    for _ in 0..400 {
        cx.run_until_parked();
        if cx.update(|cx| done(table.read(cx))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let uids: Vec<String> = cx.update(|cx| {
        table
            .read(cx)
            .rows()
            .iter()
            .map(|r| r.uid.clone())
            .collect()
    });
    panic!("never settled: {uids:?}");
}

fn uids(table: &EventsTable) -> Vec<&str> {
    let mut uids: Vec<&str> = table.rows().iter().map(|row| row.uid.as_str()).collect();
    uids.sort_unstable();
    uids
}

/// Both event APIs' fields read into the row, before the watch changes anything.
#[gpui_kit::test]
async fn both_event_apis_fields_read_into_rows(cx: &mut TestAppContext) {
    let client = connected(cx);
    let table = cx.update(|cx| ClusterRegistry::subscribe_events(cx, "kind-dev", client));
    // Either the first list (e1, e2) or, if the watch already ran, the later one.
    wait_for(cx, &table, |table| !table.rows().is_empty());
    let legacy = cx.update(|cx| {
        table
            .read(cx)
            .rows()
            .iter()
            .find(|row| row.uid == "e1")
            .cloned()
    });
    let legacy = legacy.expect("the legacy event is listed");
    assert_eq!(legacy.type_.as_deref(), Some("Warning"));
    assert_eq!(legacy.reason, "BackOff");
    assert_eq!(legacy.object_label(), "Pod/web-1");
    assert_eq!(legacy.source, "kubelet (node-a)");
    assert!(legacy.last_seen.is_some());
    cx.update(|cx| ClusterRegistry::unsubscribe_events(cx, "kind-dev"));
}

/// The v1-only event, read from a list taken before any watch ran.
#[test]
fn a_v1_only_event_reads_its_series_and_reporter() {
    let event: k8s_openapi::api::core::v1::Event = serde_json::from_value(v1_only()).unwrap();
    let row = crate::k8s::resource::events_browser::EventRow::new(&event);
    assert_eq!(row.reason, "Scheduled");
    assert_eq!(row.count, 3, "series.count");
    assert_eq!(row.source, "default-scheduler (scheduler-0)");
    assert_eq!(
        row.last_seen.map(|time| time.to_string()),
        Some("2026-10-02T09:30:00Z".to_string()),
        "series.lastObservedTime"
    );
}

/// The watch's add, update and delete reach the table.
#[gpui_kit::test]
async fn adds_updates_and_deletes_apply(cx: &mut TestAppContext) {
    let client = connected(cx);
    let table = cx.update(|cx| ClusterRegistry::subscribe_events(cx, "kind-dev", client));

    wait_for(cx, &table, |table| uids(table) == ["e1", "e3"]);
    let count = cx.update(|cx| {
        table
            .read(cx)
            .rows()
            .iter()
            .find(|row| row.uid == "e1")
            .map(|row| row.count)
    });
    assert_eq!(count, Some(5), "the MODIFIED event updated e1");
    cx.update(|cx| ClusterRegistry::unsubscribe_events(cx, "kind-dev"));
}

/// Two consumers share one table and one refcounted watch, which stops - and
/// drops its table - with the last of them.
#[gpui_kit::test]
async fn two_consumers_share_one_event_watch(cx: &mut TestAppContext) {
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
    let refcount = |cx: &mut TestAppContext| {
        cx.update(|cx| {
            cx.global::<ClusterRegistry>().sessions["kind-dev"]
                .watchers
                .refcount(&WatchKey::Events)
        })
    };

    let a = cx.update(|cx| ClusterRegistry::subscribe_events(cx, "kind-dev", client.clone()));
    let b = cx.update(|cx| ClusterRegistry::subscribe_events(cx, "kind-dev", client));
    assert_eq!(a.entity_id(), b.entity_id(), "both read one table");
    assert_eq!(refcount(cx), 2);
    assert!(cx.update(|cx| ClusterRegistry::events_watch_running(cx, "kind-dev")));

    cx.update(|cx| ClusterRegistry::unsubscribe_events(cx, "kind-dev"));
    assert!(cx.update(|cx| ClusterRegistry::events_watch_running(cx, "kind-dev")));
    cx.update(|cx| ClusterRegistry::unsubscribe_events(cx, "kind-dev"));
    assert_eq!(refcount(cx), 0);
    assert!(!cx.update(|cx| ClusterRegistry::events_watch_running(cx, "kind-dev")));
}

/// A health pause stops the Event watch without unsubscribing; a resume
/// restarts it from the session's client.
#[gpui_kit::test]
async fn a_pause_stops_and_a_resume_restarts_the_event_watch(cx: &mut TestAppContext) {
    use crate::k8s::cluster::health::HealthTransition;
    use crate::k8s::cluster::watch_registry::PauseReason;
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
    cx.update(|cx| ClusterRegistry::subscribe_events(cx, "kind-dev", client));

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(
            cx,
            "kind-dev",
            HealthTransition::Pause(PauseReason::Reconnecting),
        )
    });
    assert!(!cx.update(|cx| ClusterRegistry::events_watch_running(cx, "kind-dev")));

    cx.update(|cx| {
        ClusterRegistry::apply_health_transition(cx, "kind-dev", HealthTransition::Resume)
    });
    assert!(cx.update(|cx| ClusterRegistry::events_watch_running(cx, "kind-dev")));
    cx.update(|cx| ClusterRegistry::unsubscribe_events(cx, "kind-dev"));
}
