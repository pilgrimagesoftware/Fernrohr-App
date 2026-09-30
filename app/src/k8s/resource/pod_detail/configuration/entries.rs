//! What the Configuration tab lists: one entry per ConfigMap or Secret the pod
//! references, in first-seen order (volumes, then containers' environment,
//! then image pull secrets), each with every way the pod uses it.

use crate::k8s::object_ref::ObjectRef;
use k8s_openapi::api::core::v1::{Container, Pod};

/// One referenced ConfigMap or Secret, and the ways the pod uses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::k8s::resource::pod_detail) struct ConfigEntry {
    pub(in crate::k8s::resource::pod_detail) target: ObjectRef,
    pub(in crate::k8s::resource::pod_detail) uses: Vec<String>,
}

/// The tab's entries for `pod`.
pub(in crate::k8s::resource::pod_detail) fn entries(pod: &Pod) -> Vec<ConfigEntry> {
    let namespace = pod.metadata.namespace.as_deref().unwrap_or_default();
    let spec = pod.spec.as_ref();
    let containers: Vec<&Container> = spec
        .into_iter()
        .flat_map(|spec| {
            spec.init_containers
                .iter()
                .flatten()
                .chain(&spec.containers)
        })
        .collect();
    let mut entries: Vec<ConfigEntry> = Vec::new();
    let mut add = |target: ObjectRef, used: String| match entries
        .iter_mut()
        .find(|entry| entry.target == target)
    {
        Some(entry) => {
            if !entry.uses.contains(&used) {
                entry.uses.push(used);
            }
        }
        None => entries.push(ConfigEntry {
            target,
            uses: vec![used],
        }),
    };

    for volume in spec
        .into_iter()
        .flat_map(|spec| spec.volumes.iter().flatten())
    {
        let mut sources: Vec<ObjectRef> = Vec::new();
        if let Some(source) = &volume.config_map {
            sources.push(ObjectRef::core("ConfigMap", namespace, &source.name));
        }
        if let Some(name) = volume
            .secret
            .as_ref()
            .and_then(|s| s.secret_name.as_deref())
        {
            sources.push(ObjectRef::core("Secret", namespace, name));
        }
        for projection in volume
            .projected
            .iter()
            .flat_map(|projected| projected.sources.iter().flatten())
        {
            if let Some(source) = &projection.config_map {
                sources.push(ObjectRef::core("ConfigMap", namespace, &source.name));
            }
            if let Some(source) = &projection.secret {
                sources.push(ObjectRef::core("Secret", namespace, &source.name));
            }
        }
        let mounts: Vec<String> = containers
            .iter()
            .flat_map(|container| {
                container
                    .volume_mounts
                    .iter()
                    .flatten()
                    .filter(|mount| mount.name == volume.name)
                    .map(move |mount| {
                        format!(
                            "volume `{}` mounted at `{}` in `{}`",
                            volume.name, mount.mount_path, container.name
                        )
                    })
            })
            .collect();
        for source in sources.into_iter().filter(|source| !source.name.is_empty()) {
            if mounts.is_empty() {
                add(source.clone(), format!("volume `{}`", volume.name));
            }
            for mount in &mounts {
                add(source.clone(), mount.clone());
            }
        }
    }

    for container in &containers {
        for source in container.env_from.iter().flatten() {
            let target = source
                .config_map_ref
                .as_ref()
                .map(|r| ObjectRef::core("ConfigMap", namespace, &r.name))
                .or_else(|| {
                    source
                        .secret_ref
                        .as_ref()
                        .map(|r| ObjectRef::core("Secret", namespace, &r.name))
                });
            if let Some(target) = target.filter(|target| !target.name.is_empty()) {
                add(target, format!("`envFrom` in `{}`", container.name));
            }
        }
        for var in container.env.iter().flatten() {
            let Some(from) = &var.value_from else {
                continue;
            };
            let target = from
                .config_map_key_ref
                .as_ref()
                .map(|r| (ObjectRef::core("ConfigMap", namespace, &r.name), &r.key))
                .or_else(|| {
                    from.secret_key_ref
                        .as_ref()
                        .map(|r| (ObjectRef::core("Secret", namespace, &r.name), &r.key))
                });
            if let Some((target, key)) = target.filter(|(target, _)| !target.name.is_empty()) {
                add(
                    target,
                    format!("`{}` from key `{key}` in `{}`", var.name, container.name),
                );
            }
        }
    }

    for secret in spec
        .into_iter()
        .flat_map(|spec| spec.image_pull_secrets.iter().flatten())
        .filter(|secret| !secret.name.is_empty())
    {
        add(
            ObjectRef::core("Secret", namespace, &secret.name),
            "image pull secret".to_string(),
        );
    }
    entries
}

#[cfg(test)]
mod tests;
