//! A fake API server for tests that need a cluster to change while a panel is
//! open: it serves `get`, `list` and `watch` over objects the test writes, with a
//! `resourceVersion` bumped on every write, so a watch resumed from version `n`
//! receives exactly the writes after `n`. A watch with nothing newer to send waits
//! for the next write, then answers and closes - the client re-watches from where
//! it got to, as it would after a real server's timeout.
//!
//! Collections are addressed by path prefix and plural (`/api/v1` + `pods`,
//! `/apis/apps/v1` + `deployments`); a collection with no objects lists empty.
//!
//! Lists and watches honor a `labelSelector` of equality terms (`a=b,c==d`)
//! and a `fieldSelector` on `type`; other field selectors are ignored. A
//! request that asks for `as=PartialObjectMetadataList` in its `accept` header
//! is answered with metadata only, as a real server does. Unlike a real
//! server's, a watch doesn't send an object that stops matching a selector as
//! deleted.

use parking_lot::Mutex;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::AsyncWriteExt as _;

mod request;

use request::{Request, Selector, merge, metadata_of, not_found, same_object};

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
    /// When set, every get of a single object is refused with this.
    get_refusal: Arc<Mutex<Option<(&'static str, Value)>>>,
    /// Every create, as (plural, body); a TokenRequest as ("token", body).
    creates: Arc<Mutex<Vec<(String, Value)>>>,
    /// Every container log request's path, `/api/v1/namespaces/<ns>/pods/<pod>/log`.
    log_requests: Arc<Mutex<Vec<String>>>,
    /// When set, every list and watch is refused with this status and body.
    list_refusal: Arc<Mutex<Option<(&'static str, Value)>>>,
    /// How many lists and watches asked for metadata only.
    metadata_requests: Arc<AtomicUsize>,
    /// How many lists and watches asked for whole objects.
    full_requests: Arc<AtomicUsize>,
}

/// The token the fake answers every TokenRequest with.
pub(crate) const TEST_TOKEN: &str = "eyJhbGciOiJSUzI1NiJ9.fake-test-token";

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
            get_refusal: Arc::default(),
            creates: Arc::default(),
            log_requests: Arc::default(),
            list_refusal: Arc::default(),
            metadata_requests: Arc::default(),
            full_requests: Arc::default(),
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

    /// Every container log request so far, by path.
    pub(crate) fn log_requests(&self) -> Vec<String> {
        self.log_requests.lock().clone()
    }

    /// Every create so far, as (plural, body) - a TokenRequest as `("token", body)`.
    pub(crate) fn creates(&self) -> Vec<(String, Value)> {
        self.creates.lock().clone()
    }

    /// A create: a TokenRequest (`serviceaccounts/<name>/token`) is answered
    /// with [`TEST_TOKEN`]; anything else is stored as written and answered
    /// with itself.
    fn answer_create(&self, target: &str, sent: &str) -> (&'static str, String) {
        let path = target.split_once('?').map_or(target, |(path, _)| path);
        let sent: Value = serde_json::from_str(sent).unwrap_or(Value::Null);
        if path.ends_with("/token") {
            self.creates
                .lock()
                .push(("token".to_string(), sent.clone()));
            let mut answer = sent;
            answer["status"] = json!({ "token": TEST_TOKEN,
                "expirationTimestamp": "2026-10-14T13:00:00Z" });
            return ("201 Created", answer.to_string());
        }
        let Some(Request {
            prefix,
            plural,
            name: None,
            ..
        }) = Request::parse(path)
        else {
            return not_found();
        };
        self.creates.lock().push((plural.to_string(), sent.clone()));
        self.apply(prefix, plural, sent.clone());
        ("201 Created", sent.to_string())
    }

    /// Answers every get of a single object with `status` and `body` from now
    /// on; lists and watches still answer.
    pub(crate) fn refuse_gets(&self, status: &'static str, body: Value) {
        *self.get_refusal.lock() = Some((status, body));
    }

    /// Refuses every later list and watch with `status` (`"403 Forbidden"`)
    /// and `body`; gets of single objects still answer.
    pub(crate) fn refuse_lists(&self, status: &'static str, body: Value) {
        *self.list_refusal.lock() = Some((status, body));
    }

    /// How many lists and watches asked for metadata only, and how many for
    /// whole objects.
    pub(crate) fn list_requests(&self) -> (usize, usize) {
        (
            self.metadata_requests.load(Ordering::SeqCst),
            self.full_requests.load(Ordering::SeqCst),
        )
    }

    /// Refuses every later patch with `status` (`"409 Conflict"`) and `body`.
    pub(crate) fn refuse_patches(&self, status: &'static str, body: Value) {
        *self.patch_refusal.lock() = Some((status, body));
    }

    /// A patch of the object `target` names: refused if
    /// [`Self::refuse_patches`] says so, else recorded and its body written as
    /// the object's new state - an apply of a whole manifest, as an edit sends.
    ///
    /// A merge patch (`application/merge-patch+json`, what a bulk action
    /// sends) is merged into the stored object instead. A patch of a `scale`
    /// subresource is recorded and answered with the `Scale` it asks for.
    fn answer_patch(
        &self,
        target: &str,
        sent: &str,
        content_type: Option<&str>,
    ) -> (&'static str, String) {
        let path = target.split_once('?').map_or(target, |(path, _)| path);
        let (path, scale) = match path.strip_suffix("/scale") {
            Some(path) => (path, true),
            None => (path, false),
        };
        let Some(
            request @ Request {
                prefix,
                plural,
                name: Some(name),
                ..
            },
        ) = Request::parse(path)
        else {
            return not_found();
        };
        if let Some((status, body)) = self.patch_refusal.lock().clone() {
            return (status, body.to_string());
        }
        let sent: Value = serde_json::from_str(sent).unwrap_or(Value::Null);
        self.patches.lock().push((name.to_string(), sent.clone()));
        if scale {
            let answer = json!({ "kind": "Scale", "apiVersion": "autoscaling/v1",
                "metadata": { "name": name }, "spec": sent["spec"] });
            return ("200 OK", answer.to_string());
        }
        let object = if content_type == Some("application/merge-patch+json") {
            let stored = self.state.lock().objects.iter().find_map(|stored| {
                (request.holds(stored) && stored.object["metadata"]["name"] == name)
                    .then(|| stored.object.clone())
            });
            let Some(mut stored) = stored else {
                return not_found();
            };
            merge(&mut stored, &sent);
            stored
        } else {
            sent
        };
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
                        "PATCH" => cluster.answer_patch(
                            &request.target,
                            &request.body,
                            request.content_type.as_deref(),
                        ),
                        "POST" => cluster.answer_create(&request.target, &request.body),
                        _ => {
                            cluster
                                .answer(&request.target, request.accept.as_deref())
                                .await
                        }
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
    async fn answer(&self, target: &str, accept: Option<&str>) -> (&'static str, String) {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if path.ends_with("/log") {
            self.log_requests.lock().push(path.to_string());
        }
        let Some(request) = Request::parse(path) else {
            return not_found();
        };
        if let Some(name) = request.name {
            self.gets.fetch_add(1, Ordering::SeqCst);
            if let Some((status, body)) = self.get_refusal.lock().clone() {
                return (status, body.to_string());
            }
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
        if let Some((status, body)) = self.list_refusal.lock().clone() {
            return (status, body.to_string());
        }
        let selector = Selector::parse(query);
        let metadata_only =
            accept.is_some_and(|accept| accept.contains("as=PartialObjectMetadata"));
        let counter = if metadata_only {
            &self.metadata_requests
        } else {
            &self.full_requests
        };
        counter.fetch_add(1, Ordering::SeqCst);
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
                .filter(|write| selector.matches(&write.event["object"]))
                .map(|write| {
                    let mut event = write.event.clone();
                    if metadata_only {
                        event["object"] = metadata_of(&event["object"]);
                    }
                    format!("{event}\n")
                })
                .collect();
            return ("200 OK", body);
        }
        let state = self.state.lock();
        let items: Vec<Value> = state
            .objects
            .iter()
            .filter(|stored| request.holds(stored) && selector.matches(&stored.object))
            .map(|stored| {
                if metadata_only {
                    metadata_of(&stored.object)
                } else {
                    stored.object.clone()
                }
            })
            .collect();
        let (api_version, kind) = if metadata_only {
            ("meta.k8s.io/v1", "PartialObjectMetadataList")
        } else {
            (request.api_version(), "List")
        };
        let list = json!({
            "apiVersion": api_version, "kind": kind,
            "metadata": { "resourceVersion": state.version.to_string() },
            "items": items,
        });
        ("200 OK", list.to_string())
    }
}

#[cfg(test)]
mod tests;
