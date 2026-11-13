//! Force Sync (#354): the annotation a kind's controller watches to sync an
//! object now, for the kinds whose controller documents one. Writing it is a
//! request, not a change: the controller sees the new value and reconciles.
//!
//! - External Secrets: `force-sync` on an ExternalSecret, and
//!   `external-secrets.io/force-sync` on a ClusterExternalSecret (it refreshes
//!   every ExternalSecret it owns), with the time as Unix seconds.
//! - Flux: `reconcile.fluxcd.io/requestedAt` on any `*.toolkit.fluxcd.io`
//!   object that reconciles - not notification's Alert or Provider, which
//!   don't - with the time as RFC 3339, the `flux reconcile` request.
//! - Argo CD: `argocd.argoproj.io/refresh: normal` on an Application, which its
//!   controller removes once refreshed. A refresh re-reads the source and
//!   compares; it deploys only where the Application syncs automatically, so
//!   the action is worded as a refresh for Argo CD.

use crate::k8s::cluster::discovery::DiscoveredKind;
use serde_json::{Value, json};

/// How a kind asks to be synced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SyncAnnotation {
    pub(crate) key: &'static str,
    value: SyncValue,
    /// Who acts on it, as the confirmation names it.
    pub(crate) controller: &'static str,
    /// What the controller does on it, as the confirmation and its result
    /// word it.
    pub(crate) verb: SyncVerb,
}

/// What a controller does when its annotation changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyncVerb {
    Sync,
    Refresh,
}

impl SyncVerb {
    /// Lowercase, mid-sentence: "Ask Flux to sync …".
    pub(crate) fn word(self) -> &'static str {
        match self {
            SyncVerb::Sync => "sync",
            SyncVerb::Refresh => "refresh",
        }
    }

    /// The confirmation's title and button: "Force Sync", "Refresh".
    pub(crate) fn title(self) -> &'static str {
        match self {
            SyncVerb::Sync => "Force Sync",
            SyncVerb::Refresh => "Refresh",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyncValue {
    UnixSeconds,
    Rfc3339,
    Fixed(&'static str),
}

/// `kind`'s sync annotation, if its controller documents one.
pub(crate) fn sync_annotation(kind: &DiscoveredKind) -> Option<SyncAnnotation> {
    let (group, name) = (kind.gvk.group.as_str(), kind.gvk.kind.as_str());
    let external_secrets = |key| SyncAnnotation {
        key,
        value: SyncValue::UnixSeconds,
        controller: "External Secrets",
        verb: SyncVerb::Sync,
    };
    match (group, name) {
        ("external-secrets.io", "ExternalSecret") => Some(external_secrets("force-sync")),
        ("external-secrets.io", "ClusterExternalSecret") => {
            Some(external_secrets("external-secrets.io/force-sync"))
        }
        ("argoproj.io", "Application") => Some(SyncAnnotation {
            key: "argocd.argoproj.io/refresh",
            value: SyncValue::Fixed("normal"),
            controller: "Argo CD",
            verb: SyncVerb::Refresh,
        }),
        // Alerts and Providers configure notifications; nothing reconciles them.
        ("notification.toolkit.fluxcd.io", "Alert" | "Provider") => None,
        _ if group.ends_with(".toolkit.fluxcd.io") => Some(SyncAnnotation {
            key: "reconcile.fluxcd.io/requestedAt",
            value: SyncValue::Rfc3339,
            controller: "Flux",
            verb: SyncVerb::Sync,
        }),
        _ => None,
    }
}

impl SyncAnnotation {
    /// The merge patch that asks for a sync at `now`.
    pub(crate) fn patch(&self, now: jiff::Timestamp) -> Value {
        let value = match self.value {
            SyncValue::UnixSeconds => now.as_second().to_string(),
            SyncValue::Rfc3339 => now.to_string(),
            SyncValue::Fixed(value) => value.to_string(),
        };
        json!({ "metadata": { "annotations": { self.key: value } } })
    }
}

#[cfg(test)]
mod tests {
    use super::sync_annotation;
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use kube::core::GroupVersionKind;

    fn kind(group: &str, name: &str) -> DiscoveredKind {
        DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", name),
            plural: String::new(),
            namespaced: true,
            verbs: Default::default(),
        }
    }

    fn patch(group: &str, name: &str) -> Option<serde_json::Value> {
        let now: jiff::Timestamp = "2026-11-13T15:00:00Z".parse().expect("a timestamp");
        sync_annotation(&kind(group, name)).map(|annotation| annotation.patch(now))
    }

    #[test]
    fn each_controller_gets_its_own_annotation_and_value() {
        assert_eq!(
            patch("external-secrets.io", "ExternalSecret"),
            Some(serde_json::json!({"metadata": {"annotations": {"force-sync": "1794582000"}}}))
        );
        assert_eq!(
            patch("external-secrets.io", "ClusterExternalSecret"),
            Some(serde_json::json!({"metadata": {"annotations": {
                "external-secrets.io/force-sync": "1794582000"
            }}}))
        );
        assert_eq!(
            patch("kustomize.toolkit.fluxcd.io", "Kustomization"),
            Some(serde_json::json!({"metadata": {"annotations": {
                "reconcile.fluxcd.io/requestedAt": "2026-11-13T15:00:00Z"
            }}}))
        );
        assert_eq!(
            patch("argoproj.io", "Application"),
            Some(serde_json::json!({"metadata": {"annotations": {
                "argocd.argoproj.io/refresh": "normal"
            }}}))
        );
    }

    #[test]
    fn other_kinds_have_none() {
        assert_eq!(patch("", "ConfigMap"), None);
        assert_eq!(patch("apps", "Deployment"), None);
        assert_eq!(patch("argoproj.io", "AppProject"), None);
        assert_eq!(patch("external-secrets.io", "SecretStore"), None);
        assert_eq!(patch("notification.toolkit.fluxcd.io", "Alert"), None);
        assert_eq!(patch("notification.toolkit.fluxcd.io", "Provider"), None);
        assert!(patch("notification.toolkit.fluxcd.io", "Receiver").is_some());
    }
}

#[cfg(test)]
mod verb_tests {
    use super::{SyncVerb, sync_annotation};
    use crate::k8s::cluster::discovery::DiscoveredKind;
    use kube::core::GroupVersionKind;

    fn verb(group: &str, name: &str) -> Option<SyncVerb> {
        let kind = DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", name),
            plural: String::new(),
            namespaced: true,
            verbs: Default::default(),
        };
        sync_annotation(&kind).map(|annotation| annotation.verb)
    }

    /// Argo CD's annotation refreshes; it deploys only with auto-sync.
    #[test]
    fn argo_cd_refreshes_and_the_others_sync() {
        assert_eq!(verb("argoproj.io", "Application"), Some(SyncVerb::Refresh));
        assert_eq!(
            verb("external-secrets.io", "ExternalSecret"),
            Some(SyncVerb::Sync)
        );
        assert_eq!(
            verb("kustomize.toolkit.fluxcd.io", "Kustomization"),
            Some(SyncVerb::Sync)
        );
    }
}
