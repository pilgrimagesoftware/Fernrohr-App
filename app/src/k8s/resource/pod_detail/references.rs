//! The objects a pod names, read out of its spec as typed [`ObjectRef`]s: its
//! owners, the ConfigMaps/Secrets/claims its volumes mount, the ConfigMaps and
//! Secrets its containers read environment from, and its image pull secrets.
//!
//! Kept apart from `fields`, which decides which rows exist; this decides what
//! each row points at.

use super::model::{PodField, PodFieldValue, VolumeRow};
use crate::k8s::object_ref::ObjectRef;
use crate::ui::link::GoToEntry;
use k8s_openapi::api::core::v1::{Container, Pod, Volume};

/// One reference per owner, in the order `metadata.ownerReferences` lists
/// them - never one comma-joined line.
pub(super) fn owners(pod: &Pod) -> Vec<ObjectRef> {
    let namespace = pod.metadata.namespace.as_deref();
    pod.metadata
        .owner_references
        .iter()
        .flatten()
        .map(|owner| ObjectRef::from_owner(owner, namespace))
        .collect()
}

/// The pod's `imagePullSecrets`, each a Secret in the pod's namespace. Entries
/// with an empty name (the API allows them) are dropped rather than linked.
pub(super) fn image_pull_secrets(pod: &Pod, namespace: &str) -> Vec<ObjectRef> {
    pod.spec
        .iter()
        .flat_map(|spec| spec.image_pull_secrets.iter().flatten())
        .filter(|secret| !secret.name.is_empty())
        .map(|secret| ObjectRef::core("Secret", namespace, &secret.name))
        .collect()
}

/// One volume, named and typed - `ConfigMap`, `EmptyDir`,
/// `PersistentVolumeClaim` - covering the sources a pod uses in practice
/// rather than every `VolumeSource` variant the API defines. A source backed by
/// another object carries that object as a reference instead of folding its
/// name into the text; a projected volume carries one per ConfigMap or Secret
/// it projects.
pub(super) fn volume_row(volume: &Volume, namespace: &str) -> VolumeRow {
    let row =
        |source: &'static str, detail: Option<String>, references: Vec<ObjectRef>| VolumeRow {
            name: volume.name.clone(),
            source,
            detail,
            references,
        };
    let config_map = |name: &str| ObjectRef::core("ConfigMap", namespace, name);
    let secret = |name: &str| ObjectRef::core("Secret", namespace, name);

    if let Some(source) = &volume.config_map {
        row("ConfigMap", None, vec![config_map(&source.name)])
    } else if let Some(source) = &volume.secret {
        let references = source
            .secret_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .map(|name| vec![secret(name)])
            .unwrap_or_default();
        row("Secret", None, references)
    } else if let Some(source) = &volume.persistent_volume_claim {
        row(
            "PersistentVolumeClaim",
            None,
            vec![ObjectRef::core(
                "PersistentVolumeClaim",
                namespace,
                &source.claim_name,
            )],
        )
    } else if let Some(source) = &volume.projected {
        let references = source
            .sources
            .iter()
            .flatten()
            .filter_map(|projection| {
                projection
                    .config_map
                    .as_ref()
                    .map(|source| config_map(&source.name))
                    .or_else(|| {
                        projection
                            .secret
                            .as_ref()
                            .map(|source| secret(&source.name))
                    })
            })
            .collect();
        row("Projected", None, references)
    } else if let Some(host_path) = &volume.host_path {
        row("HostPath", Some(host_path.path.clone()), Vec::new())
    } else if volume.empty_dir.is_some() {
        row("EmptyDir", None, Vec::new())
    } else {
        row("Other", None, Vec::new())
    }
}

/// The ConfigMaps and Secrets a container reads environment variables from,
/// whole (`envFrom`) or by key (`valueFrom`), one reference per object in the
/// order first seen - so twenty keys from one ConfigMap read as one link.
pub(super) fn env_sources(container: &Container, namespace: &str) -> Vec<ObjectRef> {
    let whole = container.env_from.iter().flatten().filter_map(|source| {
        source
            .config_map_ref
            .as_ref()
            .map(|config_map| ObjectRef::core("ConfigMap", namespace, &config_map.name))
            .or_else(|| {
                source
                    .secret_ref
                    .as_ref()
                    .map(|secret| ObjectRef::core("Secret", namespace, &secret.name))
            })
    });
    let by_key = container
        .env
        .iter()
        .flatten()
        .filter_map(|var| var.value_from.as_ref())
        .filter_map(|source| {
            source
                .config_map_key_ref
                .as_ref()
                .map(|key| ObjectRef::core("ConfigMap", namespace, &key.name))
                .or_else(|| {
                    source
                        .secret_key_ref
                        .as_ref()
                        .map(|key| ObjectRef::core("Secret", namespace, &key.name))
                })
        });

    let mut references: Vec<ObjectRef> = Vec::new();
    for reference in whole.chain(by_key) {
        if !reference.name.is_empty() && !references.contains(&reference) {
            references.push(reference);
        }
    }
    references
}

/// Every reference the structured view shows, with the field it's shown in -
/// what the "Go to…" picker lists, built from the same fields the view renders
/// so the two can't disagree. Unfollowable ones are filtered out by the
/// picker's caller, not here.
pub(super) fn go_to_entries(fields: &[PodField]) -> Vec<GoToEntry> {
    let mut entries = Vec::new();
    for field in fields {
        match &field.value {
            PodFieldValue::References { targets, .. } => entries.extend(
                targets
                    .iter()
                    .map(|target| GoToEntry::new(target.clone(), field.label)),
            ),
            PodFieldValue::Volumes(volumes) => {
                for volume in volumes {
                    entries.extend(volume.references.iter().map(|target| {
                        GoToEntry::new(target.clone(), format!("Volume {}", volume.name))
                    }));
                }
            }
            PodFieldValue::Containers(containers) => {
                for container in containers {
                    entries.extend(container.env_sources.iter().map(|target| {
                        GoToEntry::new(target.clone(), format!("{} env", container.name))
                    }));
                }
            }
            PodFieldValue::Text(_)
            | PodFieldValue::Chips(_)
            | PodFieldValue::Badges(_)
            | PodFieldValue::Collapsed(_)
            | PodFieldValue::ManagedFields(_) => {}
        }
    }
    entries
}
