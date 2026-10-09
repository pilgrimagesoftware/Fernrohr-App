//! Which resource kind a client means: [`resolve`] turns a kind name, and
//! optionally its API group, into one of the context's [`DiscoveredKind`]s -
//! or an error, before anything is sent to the cluster (`agent-mcp`: an
//! unavailable kind issues no Kubernetes request).
//!
//! Every tool that names a kind resolves it here, the read tools and the
//! action tools alike, so a kind is only ever addressed the way discovery
//! reported it.

use super::error::ToolError;
use crate::k8s::cluster::discovery::DiscoveredKind;
use serde_json::{Value, json};

/// A kind as a client names it.
// UNWIRED(#189): the resource tools (section 2.2) resolve kinds with it.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct KindQuery {
    /// The kind (`Deployment`), its plural (`deployments`) or its lowercase
    /// singular (`deployment`), compared without case.
    pub(super) kind: String,
    /// The API group, when the name alone is ambiguous: `apps`, or `core` (or
    /// empty) for the core group.
    pub(super) group: Option<String>,
}

/// The one kind in `kinds` that `query` names.
// UNWIRED(#189): the resource tools (section 2.2) resolve kinds with it.
#[allow(dead_code)]
pub(super) fn resolve(
    kinds: &[DiscoveredKind],
    query: &KindQuery,
) -> Result<DiscoveredKind, ToolError> {
    let name = query.kind.trim().to_ascii_lowercase();
    let group = query.group.as_deref().map(|group| match group.trim() {
        "core" => "",
        group => group,
    });
    let mut matches: Vec<&DiscoveredKind> = kinds
        .iter()
        .filter(|kind| kind.gvk.kind.to_ascii_lowercase() == name || kind.plural == name)
        .filter(|kind| group.is_none_or(|group| kind.gvk.group == group))
        .collect();
    matches.sort();
    matches.dedup_by(|a, b| a.gvk.group == b.gvk.group);
    match matches.as_slice() {
        [] => Err(ToolError::UnsupportedKind {
            kind: query.kind.clone(),
        }),
        [kind] => Ok((*kind).clone()),
        several => Err(ToolError::AmbiguousKind {
            kind: query.kind.clone(),
            groups: several.iter().map(|kind| kind.gvk.group.clone()).collect(),
        }),
    }
}

/// How a kind reads in a tool result.
pub(super) fn kind_json(kind: &DiscoveredKind) -> Value {
    json!({
        "kind": kind.gvk.kind,
        "group": kind.gvk.group,
        "version": kind.gvk.version,
        "plural": kind.plural,
        "namespaced": kind.namespaced,
        "verbs": {
            "list": kind.verbs.list,
            "watch": kind.verbs.watch,
            "delete": kind.verbs.delete,
            "patch": kind.verbs.patch,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::test_support::discovered;

    fn kinds() -> Vec<DiscoveredKind> {
        vec![
            discovered("", "v1", "Pod", "pods", true),
            discovered("", "v1", "Event", "events", true),
            discovered("events.k8s.io", "v1", "Event", "events", true),
            discovered("apps", "v1", "Deployment", "deployments", true),
        ]
    }

    fn query(kind: &str, group: Option<&str>) -> KindQuery {
        KindQuery {
            kind: kind.to_string(),
            group: group.map(str::to_string),
        }
    }

    #[test]
    fn a_kind_is_found_by_name_plural_or_singular_in_any_case() {
        for name in ["Deployment", "deployments", "deployment", "DEPLOYMENT"] {
            let found = resolve(&kinds(), &query(name, None)).unwrap();
            assert_eq!(found.gvk.kind, "Deployment", "{name}");
        }
    }

    #[test]
    fn a_name_in_two_groups_needs_its_group() {
        assert_eq!(
            resolve(&kinds(), &query("Event", None)),
            Err(ToolError::AmbiguousKind {
                kind: "Event".into(),
                groups: vec![String::new(), "events.k8s.io".into()],
            })
        );
        let core = resolve(&kinds(), &query("events", Some("core"))).unwrap();
        assert_eq!(core.gvk.group, "");
        let newer = resolve(&kinds(), &query("Event", Some("events.k8s.io"))).unwrap();
        assert_eq!(newer.gvk.group, "events.k8s.io");
    }

    #[test]
    fn a_kind_absent_from_discovery_is_unsupported() {
        for (name, group) in [("CronJob", None), ("Pod", Some("apps"))] {
            assert_eq!(
                resolve(&kinds(), &query(name, group)),
                Err(ToolError::UnsupportedKind { kind: name.into() })
            );
        }
    }
}
