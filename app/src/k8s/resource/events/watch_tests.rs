//! `pod-events-time-window` 1.1: one pod's live events. A mock API server lists
//! the pod's events, then its first watch adds one and deletes one; every request
//! must carry the pod's field selector. A second server refuses with 403.

use super::{InvolvedObject, watch};
use crate::k8s::resource::events_browser::EventsTable;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn event(uid: &str, reason: &str) -> Value {
    json!({
        "metadata": { "uid": uid, "name": format!("web-1.{uid}"), "namespace": "shop" },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "shop", "name": "web-1", "uid": "pod-uid" },
        "type": "Warning", "reason": reason, "message": format!("{reason} happened"),
        "count": 1, "lastTimestamp": "2026-10-02T10:00:00Z",
        "source": { "component": "kubelet" },
    })
}

/// Serves `web-1`'s events: `e1` and `e2`, then a first watch that adds `e3` and
/// deletes `e2`. Answers 400 to any request without the pod's field selector, so
/// an unfiltered watch can't pass. `forbidden` answers every request 403.
async fn serve(forbidden: bool) -> std::net::SocketAddr {
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
                let selected = line.contains("involvedObject.name%3Dweb-1")
                    || line.contains("involvedObject.name=web-1");
                let (status, body) = if forbidden {
                    (
                        "403 Forbidden",
                        json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
                            "message": "events is forbidden", "reason": "Forbidden", "code": 403 })
                        .to_string(),
                    )
                } else if !selected {
                    ("400 Bad Request", "{}".to_string())
                } else if line.contains("watch=true") {
                    let body = if watched.swap(true, Ordering::SeqCst) {
                        String::new()
                    } else {
                        [
                            json!({ "type": "ADDED", "object": event("e3", "Killing") }),
                            json!({ "type": "DELETED", "object": event("e2", "Pulled") }),
                        ]
                        .iter()
                        .map(|event| format!("{event}\n"))
                        .collect()
                    };
                    ("200 OK", body)
                } else {
                    let items = if watched.load(Ordering::SeqCst) {
                        vec![event("e1", "BackOff"), event("e3", "Killing")]
                    } else {
                        vec![event("e1", "BackOff"), event("e2", "Pulled")]
                    };
                    (
                        "200 OK",
                        json!({ "apiVersion": "v1", "kind": "EventList",
                            "metadata": { "resourceVersion": "1" }, "items": items })
                        .to_string(),
                    )
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

/// Starts the watch on `web-1` against a fresh server, returning its table and task.
fn start(cx: &mut TestAppContext, forbidden: bool) -> (Entity<EventsTable>, gpui_kit::Task<()>) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let addr = handle.block_on(serve(forbidden));
    let client = {
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
            .unwrap()
    };
    cx.update(|cx| {
        let table = cx.new(|_| EventsTable::default());
        let object = InvolvedObject {
            kind: "Pod",
            namespace: Some("shop"),
            name: "web-1",
            uid: Some("pod-uid"),
        };
        let task = watch(client, &object, table.clone(), |_| {}, cx);
        (table, task)
    })
}

fn uids(cx: &mut TestAppContext, table: &Entity<EventsTable>) -> Vec<String> {
    cx.update(|cx| {
        let mut uids: Vec<String> = table
            .read(cx)
            .rows()
            .iter()
            .map(|row| row.uid.clone())
            .collect();
        uids.sort();
        uids
    })
}

fn wait_for(cx: &mut TestAppContext, mut done: impl FnMut(&mut TestAppContext) -> bool) {
    for _ in 0..400 {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("timed out");
}

/// The pod's listed events arrive, then the watch's added one appears and its
/// deleted one goes - all through the pod's own field selector.
#[gpui_kit::test]
async fn a_pods_events_list_then_follow_its_watch(cx: &mut TestAppContext) {
    let (table, _task) = start(cx, false);
    wait_for(cx, |cx| uids(cx, &table) == ["e1", "e3"]);
    let reasons: Vec<String> = cx.update(|cx| {
        table
            .read(cx)
            .rows()
            .iter()
            .map(|row| row.reason.clone())
            .collect()
    });
    assert!(reasons.contains(&"Killing".to_string()), "{reasons:?}");
}

/// A user who may not list events gets the refusal, not an endless retry.
#[gpui_kit::test]
async fn a_403_is_recorded_as_the_refusal(cx: &mut TestAppContext) {
    let (table, _task) = start(cx, true);
    wait_for(cx, |cx| cx.update(|cx| table.read(cx).refused().is_some()));
    let refused = cx.update(|cx| table.read(cx).refused().map(str::to_string));
    assert!(refused.unwrap().contains("forbidden"));
}
