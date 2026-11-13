//! The fake cluster's reading of a request: the collection a path addresses,
//! what a query's selectors pick, and the JSON shapes it answers with.

use super::{Stored, Write};
use serde_json::{Value, json};

/// A request path, split into the collection it addresses.
pub(super) struct Request<'a> {
    pub(super) prefix: &'a str,
    pub(super) namespace: Option<&'a str>,
    pub(super) plural: &'a str,
    pub(super) name: Option<&'a str>,
}

impl<'a> Request<'a> {
    /// `/api/v1/pods`, `/api/v1/namespaces/shop/pods`,
    /// `/api/v1/namespaces/shop/pods/web-1`, or - a cluster-scoped kind's
    /// single object, a Node's say - `/api/v1/nodes/node-a`, and the same
    /// under `/apis/<group>/<version>`.
    pub(super) fn parse(path: &'a str) -> Option<Self> {
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

    pub(super) fn api_version(&self) -> &str {
        self.prefix
            .trim_start_matches("/apis/")
            .trim_start_matches("/api/")
    }

    pub(super) fn holds(&self, stored: &Stored) -> bool {
        stored.prefix == self.prefix
            && stored.plural == self.plural
            && self
                .namespace
                .is_none_or(|namespace| stored.object["metadata"]["namespace"] == namespace)
    }

    pub(super) fn covers(&self, write: &Write) -> bool {
        write.prefix == self.prefix
            && write.plural == self.plural
            && self
                .namespace
                .is_none_or(|namespace| write.event["object"]["metadata"]["namespace"] == namespace)
    }
}

/// A JSON merge patch (RFC 7386): `patch`'s fields replace `target`'s, a
/// `null` removes one, and objects merge field by field.
pub(super) fn merge(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = json!({});
    }
    let target = target.as_object_mut().expect("made an object above");
    for (key, value) in patch {
        if value.is_null() {
            target.remove(key);
        } else {
            merge(target.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}

/// What a list or watch's query selects: equality label terms, and a `type`
/// field term.
#[derive(Default)]
pub(super) struct Selector {
    labels: Vec<(String, String)>,
    object_type: Option<String>,
}

impl Selector {
    pub(super) fn parse(query: &str) -> Self {
        let mut selector = Self::default();
        for pair in query.split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                continue;
            };
            let value = percent_encoding::percent_decode_str(value)
                .decode_utf8_lossy()
                .to_string();
            let terms = value.split(',').filter_map(|term| {
                let (name, wanted) = term.split_once("==").or_else(|| term.split_once('='))?;
                Some((name.trim().to_string(), wanted.trim().to_string()))
            });
            match key {
                "labelSelector" => selector.labels.extend(terms),
                "fieldSelector" => {
                    for (name, wanted) in terms {
                        if name == "type" {
                            selector.object_type = Some(wanted);
                        }
                    }
                }
                _ => {}
            }
        }
        selector
    }

    pub(super) fn matches(&self, object: &Value) -> bool {
        let labels = &object["metadata"]["labels"];
        self.labels
            .iter()
            .all(|(name, wanted)| labels[name.as_str()] == wanted.as_str())
            && self
                .object_type
                .as_ref()
                .is_none_or(|wanted| object["type"] == wanted.as_str())
    }
}

/// `object` as a metadata-only list or watch returns it.
pub(super) fn metadata_of(object: &Value) -> Value {
    json!({
        "apiVersion": "meta.k8s.io/v1",
        "kind": "PartialObjectMetadata",
        "metadata": object["metadata"],
    })
}

pub(super) fn same_object(a: &Value, b: &Value) -> bool {
    a["metadata"]["namespace"] == b["metadata"]["namespace"]
        && a["metadata"]["name"] == b["metadata"]["name"]
}

pub(super) fn not_found() -> (&'static str, String) {
    let status = json!({ "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "reason": "NotFound", "code": 404, "message": "not found" });
    ("404 Not Found", status.to_string())
}
