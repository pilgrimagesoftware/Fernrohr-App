//! A fake API server for tests that need a cluster to change while a panel is
//! open: it serves `get`, `list` and `watch` over objects the test writes, with a
//! `resourceVersion` bumped on every write, so a watch resumed from version `n`
//! receives exactly the writes after `n`. A watch with nothing newer to send waits
//! for the next write, then answers and closes - the client re-watches from where
//! it got to, as it would after a real server's timeout.
//!
//! Collections are addressed by path prefix and plural (`/api/v1` + `pods`,
//! `/apis/apps/v1` + `deployments`); a collection with no objects lists empty.

use parking_lot::Mutex;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::AsyncWriteExt as _;

/// One stored object and the collection it belongs to.
struct Stored {
    prefix: String,
    plural: String,
    object: Value,
}

/// One write, as a watch delivers it.
struct Write {
    version: u64,
    prefix: String,
    plural: String,
    event: Value,
}

#[derive(Default)]
struct State {
    version: u64,
    objects: Vec<Stored>,
    log: Vec<Write>,
}

/// The fake cluster: clone it freely, every clone writes to the same objects.
#[derive(Clone)]
pub(crate) struct FakeCluster {
    state: Arc<Mutex<State>>,
    version: Arc<tokio::sync::watch::Sender<u64>>,
    /// How many single-object `get`s the server answered - a panel that follows
    /// a watch shouldn't need more of them.
    gets: Arc<AtomicUsize>,
    /// Every delete the server was sent: the object's name and the request's
    /// options body, as JSON.
    deletes: Arc<Mutex<Vec<(String, Value)>>>,
    /// When set, every delete is refused with this status and body.
    refusal: Arc<Mutex<Option<(&'static str, Value)>>>,
    /// Every patch the server was sent: the object's name and the patch body.
    patches: Arc<Mutex<Vec<(String, Value)>>>,
    /// When set, every patch is refused with this status and body.
    patch_refusal: Arc<Mutex<Option<(&'static str, Value)>>>,
}

impl FakeCluster {
    /// Starts the server on the app's runtime and returns it with a client for it.
    pub(crate) fn start(cx: &mut gpui_kit::TestAppContext) -> (Self, kube::Client) {
        let cluster = Self {
            state: Arc::default(),
            version: Arc::new(tokio::sync::watch::channel(0).0),
            gets: Arc::default(),
            deletes: Arc::default(),
            refusal: Arc::default(),
            patches: Arc::default(),
            patch_refusal: Arc::default(),
        };
        let handle = cx.update(|cx| crate::runtime::handle(cx));
        let addr = handle.block_on(cluster.clone().serve());
        let _guard = handle.enter();
        let client =
            kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
                .unwrap();
        (cluster, client)
    }

    /// Creates or replaces `object` (matched by namespace and name) in the
    /// `prefix`/`plural` collection, as an `ADDED` or `MODIFIED` write.
    pub(crate) fn apply(&self, prefix: &str, plural: &str, mut object: Value) {
        let mut state = self.state.lock();
        state.version += 1;
        let version = state.version;
        object["metadata"]["resourceVersion"] = json!(version.to_string());
        let existing = state.objects.iter_mut().find(|stored| {
            stored.prefix == prefix
                && stored.plural == plural
                && same_object(&stored.object, &object)
        });
        let kind = match existing {
            Some(stored) => {
                stored.object = object.clone();
                "MODIFIED"
            }
            None => {
                state.objects.push(Stored {
                    prefix: prefix.into(),
                    plural: plural.into(),
                    object: object.clone(),
                });
                "ADDED"
            }
        };
        state.log.push(Write {
            version,
            prefix: prefix.into(),
            plural: plural.into(),
            event: json!({ "type": kind, "object": object }),
        });
        drop(state);
        self.version.send_replace(version);
    }

    /// Deletes the object named `name` in `namespace` from the collection.
    pub(crate) fn delete(&self, prefix: &str, plural: &str, namespace: &str, name: &str) {
        let mut state = self.state.lock();
        let Some(index) = state.objects.iter().position(|stored| {
            stored.prefix == prefix
                && stored.plural == plural
                && stored.object["metadata"]["namespace"] == namespace
                && stored.object["metadata"]["name"] == name
        }) else {
            return;
        };
        let mut object = state.objects.remove(index).object;
        state.version += 1;
        let version = state.version;
        object["metadata"]["resourceVersion"] = json!(version.to_string());
        state.log.push(Write {
            version,
            prefix: prefix.into(),
            plural: plural.into(),
            event: json!({ "type": "DELETED", "object": object }),
        });
        drop(state);
        self.version.send_replace(version);
    }

    /// How many single-object `get`s have been served.
    pub(crate) fn gets(&self) -> usize {
        self.gets.load(Ordering::SeqCst)
    }

    /// The deletes sent so far: each object's name and its delete options.
    pub(crate) fn deletes(&self) -> Vec<(String, Value)> {
        self.deletes.lock().clone()
    }

    /// Refuses every later delete with `status` (`"403 Forbidden"`) and `body`.
    pub(crate) fn refuse_deletes(&self, status: &'static str, body: Value) {
        *self.refusal.lock() = Some((status, body));
    }

    /// The patches sent so far: each object's name and its patch body.
    pub(crate) fn patches(&self) -> Vec<(String, Value)> {
        self.patches.lock().clone()
    }

    /// Refuses every later patch with `status` (`"409 Conflict"`) and `body`.
    pub(crate) fn refuse_patches(&self, status: &'static str, body: Value) {
        *self.patch_refusal.lock() = Some((status, body));
    }

    /// A patch of the object `target` names: refused if
    /// [`Self::refuse_patches`] says so, else recorded and its body written as
    /// the object's new state - an apply of a whole manifest, as an edit sends.
    fn answer_patch(&self, target: &str, sent: &str) -> (&'static str, String) {
        let path = target.split_once('?').map_or(target, |(path, _)| path);
        let Some(Request {
            prefix,
            plural,
            name: Some(name),
            ..
        }) = Request::parse(path)
        else {
            return not_found();
        };
        if let Some((status, body)) = self.patch_refusal.lock().clone() {
            return (status, body.to_string());
        }
        let object: Value = serde_json::from_str(sent).unwrap_or(Value::Null);
        self.patches.lock().push((name.to_string(), object.clone()));
        self.apply(prefix, plural, object.clone());
        ("200 OK", object.to_string())
    }

    async fn serve(self) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let cluster = self.clone();
                tokio::spawn(async move {
                    let request = crate::k8s::test_recorder::read_request(&mut stream).await;
                    let (status, body) = match request.method.as_str() {
                        "DELETE" => cluster.answer_delete(&request.target, &request.body),
                        "PATCH" => cluster.answer_patch(&request.target, &request.body),
                        _ => cluster.answer(&request.target).await,
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

    /// A delete of the object `target` names, with `sent` its options: refused
    /// if [`Self::refuse_deletes`] says so, else recorded and applied - the
    /// object leaves the collection as a `DELETED` write.
    fn answer_delete(&self, target: &str, sent: &str) -> (&'static str, String) {
        let path = target.split_once('?').map_or(target, |(path, _)| path);
        let Some(Request {
            prefix,
            namespace,
            plural,
            name: Some(name),
        }) = Request::parse(path)
        else {
            return not_found();
        };
        if let Some((status, body)) = self.refusal.lock().clone() {
            return (status, body.to_string());
        }
        self.deletes.lock().push((
            name.to_string(),
            serde_json::from_str(sent).unwrap_or(Value::Null),
        ));
        self.delete(prefix, plural, namespace.unwrap_or_default(), name);
        let done =
            json!({ "kind": "Status", "apiVersion": "v1", "status": "Success", "code": 200 });
        ("200 OK", done.to_string())
    }

    /// The status and body for a request to `target` (path and query).
    async fn answer(&self, target: &str) -> (&'static str, String) {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let Some(request) = Request::parse(path) else {
            return not_found();
        };
        if let Some(name) = request.name {
            self.gets.fetch_add(1, Ordering::SeqCst);
            let state = self.state.lock();
            return match state
                .objects
                .iter()
                .find(|stored| request.holds(stored) && stored.object["metadata"]["name"] == name)
            {
                Some(stored) => ("200 OK", stored.object.to_string()),
                None => not_found(),
            };
        }
        if query
            .split('&')
            .any(|pair| pair == "watch=true" || pair == "watch=1")
        {
            let from = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("resourceVersion="))
                .and_then(|version| version.parse::<u64>().ok())
                .unwrap_or_default();
            let mut changed = self.version.subscribe();
            let _ = changed.wait_for(|version| *version > from).await;
            let state = self.state.lock();
            let body: String = state
                .log
                .iter()
                .filter(|write| write.version > from && request.covers(write))
                .map(|write| format!("{}\n", write.event))
                .collect();
            return ("200 OK", body);
        }
        let state = self.state.lock();
        let items: Vec<&Value> = state
            .objects
            .iter()
            .filter(|stored| request.holds(stored))
            .map(|stored| &stored.object)
            .collect();
        let list = json!({
            "apiVersion": request.api_version(), "kind": "List",
            "metadata": { "resourceVersion": state.version.to_string() },
            "items": items,
        });
        ("200 OK", list.to_string())
    }
}

/// A request path, split into the collection it addresses.
struct Request<'a> {
    prefix: &'a str,
    namespace: Option<&'a str>,
    plural: &'a str,
    name: Option<&'a str>,
}

impl<'a> Request<'a> {
    /// `/api/v1/pods`, `/api/v1/namespaces/shop/pods`,
    /// `/api/v1/namespaces/shop/pods/web-1`, or - a cluster-scoped kind's
    /// single object, a Node's say - `/api/v1/nodes/node-a`, and the same
    /// under `/apis/<group>/<version>`.
    fn parse(path: &'a str) -> Option<Self> {
        let prefix_len = if path.starts_with("/api/") {
            "/api/v1".len()
        } else {
            let mut slashes = path.match_indices('/').map(|(index, _)| index);
            slashes.nth(3).unwrap_or(path.len())
        };
        let (prefix, rest) = path.split_at(prefix_len.min(path.len()));
        let parts: Vec<&str> = rest.trim_start_matches('/').split('/').collect();
        let (namespace, plural, name) = match parts.as_slice() {
            [plural] => (None, *plural, None),
            [plural, name] => (None, *plural, Some(*name)),
            ["namespaces", namespace, plural] => (Some(*namespace), *plural, None),
            ["namespaces", namespace, plural, name] => (Some(*namespace), *plural, Some(*name)),
            _ => return None,
        };
        Some(Self {
            prefix,
            namespace,
            plural,
            name,
        })
    }

    fn api_version(&self) -> &str {
        self.prefix
            .trim_start_matches("/apis/")
            .trim_start_matches("/api/")
    }

    fn holds(&self, stored: &Stored) -> bool {
        stored.prefix == self.prefix
            && stored.plural == self.plural
            && self
                .namespace
                .is_none_or(|namespace| stored.object["metadata"]["namespace"] == namespace)
    }

    fn covers(&self, write: &Write) -> bool {
        write.prefix == self.prefix
            && write.plural == self.plural
            && self
                .namespace
                .is_none_or(|namespace| write.event["object"]["metadata"]["namespace"] == namespace)
    }
}

fn same_object(a: &Value, b: &Value) -> bool {
    a["metadata"]["namespace"] == b["metadata"]["namespace"]
        && a["metadata"]["name"] == b["metadata"]["name"]
}

fn not_found() -> (&'static str, String) {
    let status = json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": "NotFound", "code": 404, "message": "not found" });
    ("404 Not Found", status.to_string())
}
