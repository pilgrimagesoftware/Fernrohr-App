//! Shared fixtures: discovered kinds, objects from JSON, a panel in a window,
//! and a fixture API server serving a fixed route table.

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::object_detail::ObjectDetailPanel;
use crate::ui::nav::{NavTarget, ObjectTarget};
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, TestAppContext, WindowHandle};
use kube::api::DynamicObject;
use kube::core::GroupVersionKind;

pub(super) fn kind(group: &str, version: &str, kind: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, version, kind),
        plural: format!("{}s", kind.to_lowercase()),
        namespaced,
    }
}

pub(super) fn replica_sets() -> DiscoveredKind {
    kind("apps", "v1", "ReplicaSet", true)
}

pub(super) fn deployments() -> DiscoveredKind {
    kind("apps", "v1", "Deployment", true)
}

pub(super) fn nodes() -> DiscoveredKind {
    kind("", "v1", "Node", false)
}

pub(super) fn object(json: serde_json::Value) -> DynamicObject {
    serde_json::from_value(json).expect("a valid object")
}

pub(super) fn target(kind: DiscoveredKind, namespace: Option<&str>, name: &str) -> ObjectTarget {
    ObjectTarget {
        kind,
        namespace: namespace.map(str::to_string),
        name: name.into(),
    }
}

/// A ReplicaSet owned by a Deployment, labelled and annotated.
pub(super) fn owned_replica_set() -> DynamicObject {
    object(serde_json::json!({
        "apiVersion": "apps/v1",
        "kind": "ReplicaSet",
        "metadata": {
            "name": "web-7d9f",
            "namespace": "staging",
            "uid": "rs-1",
            "creationTimestamp": "1970-01-01T00:00:00Z",
            "labels": { "app": "web" },
            "annotations": { "deployment.kubernetes.io/revision": "3" },
            "ownerReferences": [{
                "apiVersion": "apps/v1",
                "kind": "Deployment",
                "name": "web",
                "uid": "deploy-1",
                "controller": true,
            }],
        },
        "spec": { "replicas": 2, "selector": { "matchLabels": { "app": "web" } } },
        "status": { "replicas": 2, "readyReplicas": 1, "availableReplicas": 1 },
    }))
}

/// The panel as the app hosts it: inside a `Root`, under a view that draws the
/// dialog layer the "Go to…" picker opens in.
pub(super) struct Host {
    pub(super) panel: Entity<ObjectDetailPanel>,
}

impl gpui_kit::Render for Host {
    fn render(
        &mut self,
        _window: &mut gpui_kit::Window,
        _cx: &mut gpui_kit::Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        use gpui_kit::{ParentElement as _, Styled as _};
        gpui_kit::div().size_full().child(self.panel.clone())
    }
}

/// A panel over `target` in `kind-dev`, never connected, resolving references
/// against `kinds`.
pub(super) fn stub_panel(
    cx: &mut TestAppContext,
    target: ObjectTarget,
    kinds: Vec<DiscoveredKind>,
) -> (WindowHandle<Root>, Entity<ObjectDetailPanel>) {
    let connection =
        cx.update(|cx| cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting)));
    let mut panel = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Object(target.clone()), "kind-dev".into());
        let built =
            cx.new(|cx| ObjectDetailPanel::with_connection(target, scope, connection, kinds, cx));
        panel = Some(built.clone());
        let host = cx.new(|_| Host { panel: built });
        Root::new(host, window, cx)
    });
    (window, panel.expect("the window built its panel"))
}

/// One route: requests whose request line contains `path` get `status` and
/// `body`. The first match wins; anything unmatched is a 404 `Status`.
pub(super) struct Route {
    pub(super) path: &'static str,
    pub(super) status: &'static str,
    pub(super) body: serde_json::Value,
}

pub(super) fn not_found_status() -> serde_json::Value {
    serde_json::json!({
        "kind": "Status",
        "apiVersion": "v1",
        "status": "Failure",
        "message": "not found",
        "reason": "NotFound",
        "code": 404,
    })
}

/// Serves `routes` on a local port until the returned handle is dropped with
/// its runtime.
pub(super) async fn serve(
    routes: Vec<Route>,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let routes = std::sync::Arc::new(routes);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let routes = routes.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let request_line = request.lines().next().unwrap_or_default().to_string();
                let (status, body) = routes
                    .iter()
                    .find(|route| request_line.contains(route.path))
                    .map(|route| (route.status, route.body.to_string()))
                    .unwrap_or(("404 Not Found", not_found_status().to_string()));
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            });
        }
    });
    (addr, handle)
}
