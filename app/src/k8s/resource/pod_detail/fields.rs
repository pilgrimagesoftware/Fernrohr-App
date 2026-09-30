//! The `Pod` -> field-list projection: which rows the panel shows, in which
//! tab, in which order. The per-field formatting it leans on is `format`'s.

use super::format::{
    IpSource, format_toleration, format_volume, ips_of, managed_field_entry, non_empty,
    non_empty_map, summarize_containers,
};
use super::model::{
    ConditionBadge, DetailSection, ManagedFieldEntry, PodField, PodFieldValue, chip, condition_tone,
};
use crate::k8s::resource::pods::format_age;
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;

/// The fields the panel shows, in the order it shows them. Rows whose source is
/// absent from the pod are left out rather than rendered blank - a pod with no
/// owner references has no "Controlled By" row, it does not have an empty one.
pub fn pod_fields(pod: &Pod, now: Timestamp) -> Vec<PodField> {
    let mut fields = Vec::new();
    let mut push = |label: &'static str, section: DetailSection, value: PodFieldValue| {
        fields.push(PodField {
            label,
            section,
            value,
        });
    };

    if let Some(created) = &pod.metadata.creation_timestamp {
        let age_secs = now.duration_since(created.0).as_secs_f64() as i64;
        push(
            "Created",
            DetailSection::Overview,
            PodFieldValue::Text(format!("{} ({})", format_age(age_secs), created.0)),
        );
    }
    if let Some(name) = non_empty(&pod.metadata.name) {
        push(
            "Name",
            DetailSection::Overview,
            PodFieldValue::Text(name.to_string()),
        );
    }
    if let Some(namespace) = non_empty(&pod.metadata.namespace) {
        push(
            "Namespace",
            DetailSection::Overview,
            PodFieldValue::Link(namespace.to_string()),
        );
    }
    let containers = summarize_containers(
        pod.spec
            .as_ref()
            .map(|spec| spec.containers.as_slice())
            .unwrap_or_default(),
        pod.status.as_ref(),
    );
    if !containers.is_empty() {
        push(
            "Containers",
            DetailSection::Containers,
            PodFieldValue::Containers(containers),
        );
    }
    let init_containers = summarize_containers(
        pod.spec
            .as_ref()
            .and_then(|spec| spec.init_containers.as_deref())
            .unwrap_or_default(),
        pod.status.as_ref(),
    );
    if !init_containers.is_empty() {
        push(
            "Init Containers",
            DetailSection::Containers,
            PodFieldValue::Containers(init_containers),
        );
    }
    let volumes: Vec<String> = pod
        .spec
        .as_ref()
        .into_iter()
        .flat_map(|spec| spec.volumes.iter().flatten())
        .map(format_volume)
        .collect();
    if !volumes.is_empty() {
        push(
            "Volumes",
            DetailSection::Volumes,
            PodFieldValue::List(volumes),
        );
    }
    if let Some(labels) = non_empty_map(&pod.metadata.labels) {
        push(
            "Labels",
            DetailSection::Overview,
            PodFieldValue::Chips(labels.iter().map(|(key, value)| chip(key, value)).collect()),
        );
    }
    if let Some(annotations) = non_empty_map(&pod.metadata.annotations) {
        push(
            "Annotations",
            DetailSection::Overview,
            PodFieldValue::Chips(
                annotations
                    .iter()
                    .map(|(key, value)| chip(key, value))
                    .collect(),
            ),
        );
    }
    let owners: Vec<String> = pod
        .metadata
        .owner_references
        .iter()
        .flatten()
        .map(|owner| format!("{}/{}", owner.kind, owner.name))
        .collect();
    if !owners.is_empty() {
        push(
            "Controlled By",
            DetailSection::Overview,
            PodFieldValue::Link(owners.join(", ")),
        );
    }
    let managed_fields: Vec<ManagedFieldEntry> = pod
        .metadata
        .managed_fields
        .iter()
        .flatten()
        .map(managed_field_entry)
        .collect();
    if !managed_fields.is_empty() {
        push(
            "Managed Fields",
            DetailSection::ManagedFields,
            PodFieldValue::ManagedFields(managed_fields),
        );
    }
    if let Some(phase) = pod
        .status
        .as_ref()
        .and_then(|status| non_empty(&status.phase))
    {
        push(
            "Status",
            DetailSection::Overview,
            PodFieldValue::Text(phase.to_string()),
        );
    }
    if let Some(node) = pod
        .spec
        .as_ref()
        .and_then(|spec| non_empty(&spec.node_name))
    {
        push(
            "Node",
            DetailSection::Overview,
            PodFieldValue::Link(node.to_string()),
        );
    }
    if let Some(ips) = ips_of(pod, IpSource::Host) {
        push(
            "Host IPs",
            DetailSection::Overview,
            PodFieldValue::Text(ips),
        );
    }
    if let Some(ips) = ips_of(pod, IpSource::Pod) {
        push("Pod IPs", DetailSection::Overview, PodFieldValue::Text(ips));
    }
    if let Some(account) = pod
        .spec
        .as_ref()
        .and_then(|spec| non_empty(&spec.service_account_name))
    {
        push(
            "Service Account",
            DetailSection::Overview,
            PodFieldValue::Link(account.to_string()),
        );
    }
    if let Some(qos) = pod
        .status
        .as_ref()
        .and_then(|status| non_empty(&status.qos_class))
    {
        push(
            "QoS Class",
            DetailSection::Overview,
            PodFieldValue::Text(qos.to_string()),
        );
    }
    if let Some(grace) = pod
        .spec
        .as_ref()
        .and_then(|spec| spec.termination_grace_period_seconds)
    {
        push(
            "Termination Grace Period",
            DetailSection::Overview,
            PodFieldValue::Text(format_age(grace)),
        );
    }
    let tolerations: Vec<String> = pod
        .spec
        .as_ref()
        .into_iter()
        .flat_map(|spec| spec.tolerations.iter().flatten())
        .map(format_toleration)
        .collect();
    if !tolerations.is_empty() {
        push(
            "Tolerations",
            DetailSection::Overview,
            PodFieldValue::Collapsed(tolerations),
        );
    }
    let conditions: Vec<ConditionBadge> = pod
        .status
        .as_ref()
        .into_iter()
        .flat_map(|status| status.conditions.iter().flatten())
        .map(|condition| ConditionBadge {
            condition: condition.type_.clone(),
            status: condition.status.clone(),
            tone: condition_tone(&condition.status),
        })
        .collect();
    if !conditions.is_empty() {
        push(
            "Conditions",
            DetailSection::Overview,
            PodFieldValue::Badges(conditions),
        );
    }

    fields
}
