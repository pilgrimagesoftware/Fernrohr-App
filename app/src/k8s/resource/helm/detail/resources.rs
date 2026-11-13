//! The Resources tab (helm-releases section 6, D7): every object the deployed
//! revision's manifest declares, each a `resource-links` reference to its own
//! panel, marked when it doesn't exist in the cluster.
//!
//! The manifest is fetched and parsed off the UI thread, then each object is
//! checked with one GET - when the tab first opens and again on a new
//! deployed revision. Only object references leave the manifest; its text is
//! never kept. Hook resources aren't in a release's manifest, so they aren't
//! listed, which matches `helm get manifest`. Drawing the tab is `render`'s.

use super::panel::HelmReleasePanel;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::helm::decode_release;
use crate::ui::link::FollowReference;
use crate::ui::nav::OpenMode;
use gpui_kit::*;
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};
use serde::Deserialize as _;

/// Whether a listed object exists in the cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presence {
    Present,
    /// A 404: the manifest declares it, the cluster doesn't have it.
    Missing,
    /// Not checked: a kind discovery doesn't know, or the check failed for
    /// another reason than not found.
    Unchecked,
}

/// One object the manifest declares, and whether it exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResourceRow {
    pub(crate) target: ObjectRef,
    pub(crate) presence: Presence,
}

/// The tab's data, for the deployed revision Secret it was read from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Resources {
    #[default]
    NotFetched,
    Loading {
        source: (String, String),
    },
    Ready {
        source: (String, String),
        rows: Vec<ResourceRow>,
    },
    Unreadable {
        source: (String, String),
        reason: String,
    },
}

impl Resources {
    /// The deployed revision Secret and resourceVersion this was read from.
    fn source(&self) -> Option<&(String, String)> {
        match self {
            Self::NotFetched => None,
            Self::Loading { source }
            | Self::Ready { source, .. }
            | Self::Unreadable { source, .. } => Some(source),
        }
    }
}

/// The objects `manifest` declares, in its order: each document with a
/// `kind` and a `metadata.name` - and each item of a `List` - with its
/// namespace, else `namespace`, the release's. Reading stops at a document
/// that doesn't parse: after an error the YAML reader yields only errors,
/// endlessly, so nothing after it can be read.
pub(crate) fn parse_manifest(manifest: &str, namespace: &str) -> Vec<ObjectRef> {
    let mut targets = Vec::new();
    for document in serde_yaml_ng::Deserializer::from_str(manifest) {
        let Ok(value) = serde_yaml_ng::Value::deserialize(document) else {
            break;
        };
        collect(&value, namespace, &mut targets);
    }
    targets
}

fn collect(value: &serde_yaml_ng::Value, namespace: &str, into: &mut Vec<ObjectRef>) {
    let text = |value: &serde_yaml_ng::Value, key: &str| {
        value
            .get(key)
            .and_then(serde_yaml_ng::Value::as_str)
            .map(str::to_string)
    };
    let Some(kind) = text(value, "kind") else {
        return;
    };
    if kind == "List" {
        for item in value
            .get("items")
            .and_then(|items| items.as_sequence())
            .into_iter()
            .flatten()
        {
            collect(item, namespace, into);
        }
        return;
    }
    let (Some(api_version), Some(metadata)) = (text(value, "apiVersion"), value.get("metadata"))
    else {
        return;
    };
    let Some(name) = text(metadata, "name") else {
        return;
    };
    let namespace = text(metadata, "namespace").unwrap_or_else(|| namespace.to_string());
    into.push(ObjectRef::from_api_version(
        &api_version,
        kind,
        Some(namespace),
        name,
    ));
}

/// `target`'s kind as discovery reports it, if it knows it.
fn kind_of<'a>(target: &ObjectRef, kinds: &'a [DiscoveredKind]) -> Option<&'a DiscoveredKind> {
    kinds
        .iter()
        .find(|kind| kind.gvk.group == target.group && kind.gvk.kind == target.kind)
}

/// `target` with its namespace dropped when discovery reports its kind as
/// cluster-scoped - a ClusterRole has none, whatever the release's.
pub(crate) fn scoped(mut target: ObjectRef, kinds: &[DiscoveredKind]) -> ObjectRef {
    if kind_of(&target, kinds).is_some_and(|kind| !kind.namespaced) {
        target.namespace = None;
    }
    target
}

/// Fetches revision Secret `secret` in `namespace`, parses its manifest and
/// checks each object - off the UI thread - then hands the rows to `done` on
/// `panel`.
fn fetch(
    client: Client,
    namespace: String,
    secret: String,
    kinds: Vec<DiscoveredKind>,
    panel: WeakEntity<HelmReleasePanel>,
    done: impl FnOnce(
        &mut HelmReleasePanel,
        Result<Vec<ResourceRow>, String>,
        &mut Context<HelmReleasePanel>,
    ) + 'static,
    cx: &mut App,
) {
    let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let rows = read(client, &namespace, &secret, &kinds).await;
        let _ = tx.send(rows).await;
    });
    cx.spawn(async move |cx| {
        let mut done = Some(done);
        crate::runtime::drain(rx, |rows| {
            if let Some(done) = done.take() {
                let _ = panel.update(cx, |panel, cx| done(panel, rows, cx));
            }
        })
        .await;
    })
    .detach();
}

async fn read(
    client: Client,
    namespace: &str,
    secret: &str,
    kinds: &[DiscoveredKind],
) -> Result<Vec<ResourceRow>, String> {
    let api: Api<Secret> = Api::namespaced(client.clone(), namespace);
    let secret = match api.get(secret).await {
        Ok(secret) => secret,
        Err(kube::Error::Api(status)) if !status.message.is_empty() => return Err(status.message),
        Err(_) => return Err("the deployed revision couldn't be fetched".into()),
    };
    let release_namespace = namespace.to_string();
    let targets = tokio::task::spawn_blocking(move || {
        let payload = secret.data.as_ref()?.get("release")?.0.clone();
        let release = decode_release(&payload).ok()?;
        Some(parse_manifest(
            release.manifest.expose()?,
            &release_namespace,
        ))
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| "the deployed revision's manifest couldn't be read".to_string())?;
    let checks = targets.into_iter().map(|target| {
        let client = client.clone();
        let target = scoped(target, kinds);
        let kind = kind_of(&target, kinds).cloned();
        async move {
            let presence = match kind {
                Some(kind) => presence(client, &kind, &target).await,
                None => Presence::Unchecked,
            };
            ResourceRow { target, presence }
        }
    });
    Ok(futures_util::future::join_all(checks).await)
}

/// Whether `target`, of `kind`, exists: a GET, its 404 meaning missing.
async fn presence(client: Client, kind: &DiscoveredKind, target: &ObjectRef) -> Presence {
    let api =
        crate::k8s::resource::resource_actions::api_for(client, kind, target.namespace.as_deref());
    match api.get(&target.name).await {
        Ok(_) => Presence::Present,
        Err(kube::Error::Api(status)) if status.code == 404 => Presence::Missing,
        Err(_) => Presence::Unchecked,
    }
}

impl HelmReleasePanel {
    /// The listed objects, once read.
    pub(crate) fn resource_rows(&self) -> &[ResourceRow] {
        match &self.resources {
            Resources::Ready { rows, .. } => rows,
            _ => &[],
        }
    }

    /// Reads the deployed revision's objects, unless they were read from it
    /// already: when the tab first shows, and again on a new deployed
    /// revision. Waits for discovery, which decides each object's scope.
    pub(super) fn fetch_resources(&mut self, cx: &mut Context<Self>) {
        let history = self.history(cx);
        let Some(deployed) = history
            .iter()
            .find(|revision| revision.status == "deployed")
            .or(history.first())
        else {
            return;
        };
        let source = (deployed.secret.clone(), deployed.resource_version.clone());
        if self.resources.source() == Some(&source) {
            return;
        }
        let (Some(client), Some(kinds)) = (
            self.client(cx),
            self.discovery.read(cx).kinds().map(<[_]>::to_vec),
        ) else {
            return;
        };
        self.resources = Resources::Loading {
            source: source.clone(),
        };
        fetch(
            client,
            self.key.namespace.clone(),
            source.0.clone(),
            kinds,
            cx.weak_entity(),
            move |this, rows, cx| {
                if this.resources.source() != Some(&source) {
                    return;
                }
                this.resources = match rows {
                    Ok(rows) => Resources::Ready { source, rows },
                    Err(reason) => Resources::Unreadable { source, reason },
                };
                let last = this.resource_rows().len().saturating_sub(1);
                this.selected_resource = this.selected_resource.min(last);
                cx.notify();
            },
            cx,
        );
    }

    /// Moves the Resources selection `by` rows (negative is up).
    pub(super) fn move_resource_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let count = self.resource_rows().len() as isize;
        if count == 0 {
            return;
        }
        self.selected_resource =
            (self.selected_resource as isize + by).clamp(0, count - 1) as usize;
        cx.notify();
    }

    /// Follows the selected object to its panel - a `resource-links`
    /// reference, so a missing one opens the not-found panel and a kind
    /// discovery doesn't know opens nothing.
    pub(super) fn open_resource(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.resource_rows().get(self.selected_resource) else {
            return;
        };
        window.dispatch_action(
            Box::new(FollowReference {
                context_name: self.scope.context_name.clone(),
                target: row.target.clone(),
                mode: OpenMode::Foreground,
            }),
            cx,
        );
    }
}

#[cfg(test)]
mod tests;
mod view;
