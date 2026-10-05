//! `unwatchable-kinds`: a kind that can be listed but not watched is polled into
//! the shared table, and one that can't be listed says so. Served by a mock API
//! server whose component-status list carries no `resourceVersion`, as the real
//! `componentstatuses` list does.

use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::discovery::{DiscoveredKind, KindVerbs};
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::resource::object_list::{ListMode, ObjectsTable};
use gpui_kit::{Entity, TestAppContext};
use kube::core::GroupVersionKind;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn component_statuses(verbs: KindVerbs) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", "ComponentStatus"),
        plural: "componentstatuses".into(),
        namespaced: false,
        verbs,
    }
}

/// A mock API server. Its `componentstatuses` list has no `resourceVersion`
/// and grows by one object per list served, so a re-list shows; a watch
/// request gets an empty response. Returns its address and the count of lists
/// served.
async fn serve() -> (std::net::SocketAddr, Arc<AtomicUsize>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let lists = Arc::new(AtomicUsize::new(0));
    let served = lists.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let served = served.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let line = String::from_utf8_lossy(&buffer[..read])
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let body = if line.contains("watch=true") {
                    String::new()
                } else {
                    let count = served.fetch_add(1, Ordering::SeqCst) + 1;
                    let items: Vec<_> = (1..=count)
                        .map(|n| json!({ "metadata": { "uid": format!("cs{n}"), "name": format!("etcd-{n}") } }))
                        .collect();
                    // No `metadata.resourceVersion`, as the real list has none.
                    json!({ "apiVersion": "v1", "kind": "ComponentStatusList", "metadata": {}, "items": items })
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
    (addr, lists)
}

fn connected(cx: &mut TestAppContext) -> (kube::Client, Arc<AtomicUsize>) {
    cx.executor().allow_parking();
    cx.update(crate::runtime::init);
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (addr, lists) = handle.block_on(serve());
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
    (client, lists)
}

/// Polls until `done` holds for `table`.
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
    let rows = cx.update(|cx| table.read(cx).rows().len());
    panic!("never settled: {rows} rows");
}

fn polled(table: &ObjectsTable) -> bool {
    matches!(table.mode(), ListMode::Polled { .. })
}

/// The watchable-by-discovery case the user hit: the list has no
/// `resourceVersion`, so the watch can't start. It falls back to polling, and
/// the list's rows appear instead of an empty table.
#[gpui_kit::test]
async fn a_list_without_a_resource_version_falls_back_to_polling(cx: &mut TestAppContext) {
    let (client, _lists) = connected(cx);
    let kind = component_statuses(KindVerbs::default());

    let table = cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client, &kind));

    wait_for(cx, &table, |table| {
        polled(table) && !table.rows().is_empty()
    });
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
}

/// A kind discovery offers no `watch` for is polled from the start. A refresh
/// re-lists it now rather than in 30s, and the new list's rows are applied.
#[gpui_kit::test]
async fn a_kind_without_watch_is_polled_and_refresh_relists(cx: &mut TestAppContext) {
    let (client, lists) = connected(cx);
    let kind = component_statuses(KindVerbs {
        list: true,
        watch: false,
        delete: true,
    });

    let table = cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client, &kind));
    wait_for(cx, &table, |table| polled(table) && table.rows().len() == 1);
    assert_eq!(
        lists.load(Ordering::SeqCst),
        1,
        "listed once, no watch attempted"
    );

    cx.update(|cx| table.read(cx).request_refresh());
    wait_for(cx, &table, |table| table.rows().len() == 2);
    assert_eq!(lists.load(Ordering::SeqCst), 2, "the refresh re-listed");
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
}

/// A kind with no `list` verb is marked unlistable and never requested.
#[gpui_kit::test]
async fn a_kind_without_list_is_unlistable(cx: &mut TestAppContext) {
    let (client, lists) = connected(cx);
    let kind = component_statuses(KindVerbs {
        list: false,
        watch: false,
        delete: true,
    });

    let table = cx.update(|cx| ClusterRegistry::subscribe_kind(cx, "kind-dev", client, &kind));
    cx.run_until_parked();

    assert!(cx.update(|cx| matches!(table.read(cx).mode(), ListMode::Unlistable)));
    assert_eq!(lists.load(Ordering::SeqCst), 0, "nothing was requested");
    cx.update(|cx| ClusterRegistry::unsubscribe_kind(cx, "kind-dev", &kind));
}
