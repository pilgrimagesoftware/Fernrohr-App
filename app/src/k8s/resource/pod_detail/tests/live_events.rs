//! `pod-events-time-window` 1.2: the Events tab follows the pod's live watch. A
//! pod detail panel in a window, connected to a mock API server that serves the
//! pod once and then a watch adding an event: the new event shows without the
//! panel being reopened or the pod refetched, and closing the panel ends the
//! watch.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::resource::pod_detail::{DetailView, PodDetailPanel};
use crate::ui::nav::{NavTarget, PodRef};
use crate::ui::panel_title::PanelScope;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext, WeakEntity};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn event(uid: &str, reason: &str) -> Value {
    json!({
        "metadata": { "uid": uid, "name": format!("web-1.{uid}"), "namespace": "shop" },
        "involvedObject": { "kind": "Pod", "apiVersion": "v1", "namespace": "shop", "name": "web-1", "uid": "pod-uid" },
        "type": "Normal", "reason": reason, "message": format!("{reason} happened"),
        // Seen just now, so the Events tab's default one-hour window shows it.
        "count": 1, "lastTimestamp": jiff::Timestamp::now().to_string(),
    })
}

/// Serves `shop/web-1` (counting its fetches), its events list with `Pulled`, and
/// on the first watch a `Started` event added later - in every list after that.
async fn serve(pod_fetches: Arc<AtomicUsize>) -> std::net::SocketAddr {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let watched = Arc::new(AtomicBool::new(false));
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let (watched, pod_fetches) = (watched.clone(), pod_fetches.clone());
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let line = String::from_utf8_lossy(&buffer[..read])
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let body = if line.starts_with("GET /api/v1/namespaces/shop/pods/web-1 ") {
                    pod_fetches.fetch_add(1, Ordering::SeqCst);
                    json!({ "apiVersion": "v1", "kind": "Pod",
                        "metadata": { "name": "web-1", "namespace": "shop", "uid": "pod-uid" } })
                    .to_string()
                } else if line.contains("watch=true") {
                    if watched.swap(true, Ordering::SeqCst) {
                        String::new()
                    } else {
                        format!(
                            "{}\n",
                            json!({ "type": "ADDED", "object": event("e2", "Started") })
                        )
                    }
                } else {
                    // Once the watch has added `Started`, a relist shows it too.
                    let items = if watched.load(Ordering::SeqCst) {
                        vec![event("e1", "Pulled"), event("e2", "Started")]
                    } else {
                        vec![event("e1", "Pulled")]
                    };
                    json!({ "apiVersion": "v1", "kind": "EventList",
                        "metadata": { "resourceVersion": "1" }, "items": items })
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

fn reasons(vcx: &mut VisualTestContext, panel: &Entity<PodDetailPanel>) -> Vec<String> {
    vcx.update(|_, cx| {
        let summaries = panel
            .read(cx)
            .events_view(jiff::Timestamp::now(), cx)
            .and_then(|view| view.events.ok())
            .unwrap_or_default();
        let mut reasons: Vec<String> = summaries.into_iter().map(|event| event.reason).collect();
        reasons.sort();
        reasons
    })
}

#[gpui_kit::test]
async fn a_new_event_shows_live_and_closing_the_panel_ends_the_watch(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let pod_fetches = Arc::new(AtomicUsize::new(0));
    let addr = handle.block_on(serve(pod_fetches.clone()));
    let client = {
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
            .unwrap()
    };
    let mut built = None;
    let window = cx.add_window(|_window, cx| {
        let connection =
            cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)));
        let pod = PodRef {
            namespace: "shop".into(),
            name: "web-1".into(),
        };
        let scope = PanelScope::new(NavTarget::pod("shop", "web-1"), "demo".into());
        let panel = cx.new(|cx| {
            PodDetailPanel::with_connection(pod, scope, DetailView::Structured, connection, cx)
        });
        built = Some(panel.clone());
        gpui_kit::component::Root::new(panel, _window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);

    let mut seen = Vec::new();
    for _ in 0..400 {
        vcx.run_until_parked();
        seen = reasons(&mut vcx, &panel);
        if seen == ["Pulled", "Started"] {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        seen,
        ["Pulled", "Started"],
        "the watched event joined the listed one"
    );
    assert_eq!(
        pod_fetches.load(Ordering::SeqCst),
        1,
        "without refetching the pod"
    );

    // Closing the panel drops its watch, and with it the events table.
    let table: WeakEntity<_> = vcx.update(|_, cx| {
        panel
            .read(cx)
            .events
            .as_ref()
            .expect("watching")
            .table
            .downgrade()
    });
    drop(panel);
    vcx.update(|window, _| window.remove_window());
    vcx.run_until_parked();
    assert!(table.upgrade().is_none(), "the watch ended with the panel");
}
