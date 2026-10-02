//! A Node's sections: where it is reachable, what it offers, how it's doing,
//! what it runs, and what it repels.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use super::common::{condition_badges, non_empty, quantities};
use k8s_openapi::api::core::v1::Node;

pub(super) fn sections(node: &Node) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let status = node.status.as_ref();
    let spec = node.spec.as_ref();

    let addresses: Vec<String> = status
        .into_iter()
        .flat_map(|status| status.addresses.iter().flatten())
        .map(|address| format!("{}: {}", address.type_, address.address))
        .collect();
    if !addresses.is_empty() {
        fields.push(ObjectField::new("Addresses", FieldValue::Lines(addresses)));
    }
    if let Some(capacity) = quantities(status.and_then(|status| status.capacity.as_ref())) {
        fields.push(ObjectField::new("Capacity", capacity));
    }
    if let Some(allocatable) = quantities(status.and_then(|status| status.allocatable.as_ref())) {
        fields.push(ObjectField::new("Allocatable", allocatable));
    }
    // `Ready` is the one condition that is good news when it holds; the
    // pressure conditions are good news when they don't.
    let conditions = status
        .into_iter()
        .flat_map(|status| status.conditions.iter().flatten())
        .map(|condition| (condition.type_.as_str(), condition.status.as_str()));
    if let Some(badges) = condition_badges(conditions, |condition| condition != "Ready") {
        fields.push(ObjectField::new("Conditions", badges));
    }
    if let Some(info) = status.and_then(|status| status.node_info.as_ref()) {
        fields.push(ObjectField::new(
            "Node Info",
            FieldValue::Lines(vec![
                format!("Kubelet: {}", info.kubelet_version),
                format!("OS: {} ({})", info.os_image, info.operating_system),
                format!("Kernel: {}", info.kernel_version),
                format!("Container runtime: {}", info.container_runtime_version),
                format!("Architecture: {}", info.architecture),
            ]),
        ));
    }
    if spec.and_then(|spec| spec.unschedulable) == Some(true) {
        fields.push(ObjectField::text("Unschedulable", "Yes (cordoned)"));
    }
    if let Some(cidr) = non_empty(spec.and_then(|spec| spec.pod_cidr.as_deref())) {
        fields.push(ObjectField::text("Pod CIDR", cidr));
    }
    let taints: Vec<String> = spec
        .into_iter()
        .flat_map(|spec| spec.taints.iter().flatten())
        .map(|taint| match non_empty(taint.value.as_deref()) {
            Some(value) => format!("{}={value}: {}", taint.key, taint.effect),
            None => format!("{}: {}", taint.key, taint.effect),
        })
        .collect();
    if !taints.is_empty() {
        fields.push(ObjectField::new("Taints", FieldValue::Lines(taints)));
    }

    vec![ObjectSection::new("Node", fields)]
}
