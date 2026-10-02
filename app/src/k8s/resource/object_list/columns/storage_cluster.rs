//! Storage, Cluster and Access Control kinds' list columns
//! (`standard-resource-panels` 2.4), as `kubectl get` shows them:
//! PersistentVolumeClaim, PersistentVolume, StorageClass, Node, Namespace,
//! ServiceAccount, RoleBinding and ClusterRoleBinding.

use super::{Cell, ColumnDef, KindColumns, typed_cells};
use k8s_openapi::api::core::v1::{
    Namespace, Node, PersistentVolume, PersistentVolumeClaim, ServiceAccount,
};
use k8s_openapi::api::rbac::v1::{ClusterRoleBinding, RoleBinding, RoleRef, Subject};
use k8s_openapi::api::storage::v1::StorageClass;
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use std::collections::BTreeMap;

const fn column(id: &'static str, title: &'static str, width: f32) -> ColumnDef {
    ColumnDef { id, title, width }
}

const STATUS: ColumnDef = column("status", "Status", 90.);
const CAPACITY: ColumnDef = column("capacity", "Capacity", 80.);
const ACCESS_MODES: ColumnDef = column("access_modes", "Access modes", 100.);
const STORAGE_CLASS: ColumnDef = column("storage_class", "Storage class", 120.);
const RECLAIM_POLICY: ColumnDef = column("reclaim_policy", "Reclaim policy", 110.);
const ROLE: ColumnDef = column("role", "Role", 200.);
const SUBJECTS: ColumnDef = column("subjects", "Subjects", 260.);

/// The prefix of the labels that name a Node's roles.
const NODE_ROLE_LABEL: &str = "node-role.kubernetes.io/";

/// `storage` as `kubectl` shows it, `10Gi`.
fn storage(capacity: Option<&BTreeMap<String, Quantity>>) -> Cell {
    Cell::text(
        capacity
            .and_then(|capacity| capacity.get("storage"))
            .map(|quantity| quantity.0.clone())
            .unwrap_or_default(),
    )
}

/// Access modes abbreviated as `kubectl` prints them: `RWO,ROX`.
fn access_modes(modes: Option<&Vec<String>>) -> Cell {
    let abbreviated: Vec<&str> = modes
        .into_iter()
        .flatten()
        .map(|mode| match mode.as_str() {
            "ReadWriteOnce" => "RWO",
            "ReadOnlyMany" => "ROX",
            "ReadWriteMany" => "RWX",
            "ReadWriteOncePod" => "RWOP",
            other => other,
        })
        .collect();
    Cell::text(abbreviated.join(","))
}

pub(super) static PERSISTENT_VOLUME_CLAIM: KindColumns = KindColumns {
    columns: &[
        STATUS,
        column("volume", "Volume", 200.),
        CAPACITY,
        ACCESS_MODES,
        STORAGE_CLASS,
    ],
    cells: |object| {
        typed_cells::<PersistentVolumeClaim>(object, |claim| {
            let spec = claim.spec.as_ref();
            let status = claim.status.as_ref();
            vec![
                Cell::text(
                    status
                        .and_then(|status| status.phase.clone())
                        .unwrap_or_default(),
                ),
                Cell::text(
                    spec.and_then(|spec| spec.volume_name.clone())
                        .unwrap_or_default(),
                ),
                storage(status.and_then(|status| status.capacity.as_ref())),
                access_modes(status.and_then(|status| status.access_modes.as_ref())),
                Cell::text(
                    spec.and_then(|spec| spec.storage_class_name.clone())
                        .unwrap_or_default(),
                ),
            ]
        })
    },
};

pub(super) static PERSISTENT_VOLUME: KindColumns = KindColumns {
    columns: &[
        CAPACITY,
        ACCESS_MODES,
        RECLAIM_POLICY,
        STATUS,
        column("claim", "Claim", 200.),
        STORAGE_CLASS,
    ],
    cells: |object| {
        typed_cells::<PersistentVolume>(object, |volume| {
            let spec = volume.spec.as_ref();
            // `namespace/name`, as `kubectl` names a bound volume's claim.
            let claim = spec
                .and_then(|spec| spec.claim_ref.as_ref())
                .and_then(|claim| {
                    let name = claim.name.as_deref()?;
                    Some(match claim.namespace.as_deref() {
                        Some(namespace) => format!("{namespace}/{name}"),
                        None => name.to_string(),
                    })
                })
                .unwrap_or_default();
            vec![
                storage(spec.and_then(|spec| spec.capacity.as_ref())),
                access_modes(spec.and_then(|spec| spec.access_modes.as_ref())),
                Cell::text(
                    spec.and_then(|spec| spec.persistent_volume_reclaim_policy.clone())
                        .unwrap_or_default(),
                ),
                Cell::text(
                    volume
                        .status
                        .as_ref()
                        .and_then(|status| status.phase.clone())
                        .unwrap_or_default(),
                ),
                Cell::text(claim),
                Cell::text(
                    spec.and_then(|spec| spec.storage_class_name.clone())
                        .unwrap_or_default(),
                ),
            ]
        })
    },
};

pub(super) static STORAGE_CLASS_COLUMNS: KindColumns = KindColumns {
    columns: &[
        column("provisioner", "Provisioner", 200.),
        RECLAIM_POLICY,
        column("volume_binding_mode", "Volume binding mode", 160.),
    ],
    cells: |object| {
        typed_cells::<StorageClass>(object, |class| {
            // The API's defaults, which `kubectl` shows for an unset field.
            vec![
                Cell::text(class.provisioner.clone()),
                Cell::text(class.reclaim_policy.as_deref().unwrap_or("Delete")),
                Cell::text(class.volume_binding_mode.as_deref().unwrap_or("Immediate")),
            ]
        })
    },
};

/// `Ready` or `NotReady` by the Ready condition (`Unknown` without one), with
/// `,SchedulingDisabled` for a cordoned Node - as `kubectl get nodes` shows it.
fn node_status(node: &Node) -> String {
    let ready = node
        .status
        .iter()
        .flat_map(|status| status.conditions.iter().flatten())
        .find(|condition| condition.type_ == "Ready")
        .map(|condition| condition.status.as_str());
    let mut status = match ready {
        Some("True") => "Ready",
        Some(_) => "NotReady",
        None => "Unknown",
    }
    .to_string();
    if node.spec.as_ref().and_then(|spec| spec.unschedulable) == Some(true) {
        status.push_str(",SchedulingDisabled");
    }
    status
}

pub(super) static NODE: KindColumns = KindColumns {
    columns: &[
        STATUS,
        column("roles", "Roles", 120.),
        column("version", "Version", 90.),
        column("internal_ip", "Internal IP", 120.),
    ],
    cells: |object| {
        typed_cells::<Node>(object, |node| {
            let roles: Vec<&str> = node
                .metadata
                .labels
                .iter()
                .flatten()
                .filter_map(|(label, _)| label.strip_prefix(NODE_ROLE_LABEL))
                .filter(|role| !role.is_empty())
                .collect();
            let status = node.status.as_ref();
            let internal_ip = status
                .and_then(|status| status.addresses.as_ref())
                .into_iter()
                .flatten()
                .find(|address| address.type_ == "InternalIP")
                .map(|address| address.address.clone())
                .unwrap_or_default();
            vec![
                Cell::text(node_status(node)),
                Cell::text(if roles.is_empty() {
                    "<none>".to_string()
                } else {
                    roles.join(",")
                }),
                Cell::text(
                    status
                        .and_then(|status| status.node_info.as_ref())
                        .map(|info| info.kubelet_version.clone())
                        .unwrap_or_default(),
                ),
                Cell::text(internal_ip),
            ]
        })
    },
};

pub(super) static NAMESPACE: KindColumns = KindColumns {
    columns: &[STATUS],
    cells: |object| {
        typed_cells::<Namespace>(object, |namespace| {
            vec![Cell::text(
                namespace
                    .status
                    .as_ref()
                    .and_then(|status| status.phase.clone())
                    .unwrap_or_default(),
            )]
        })
    },
};

pub(super) static SERVICE_ACCOUNT: KindColumns = KindColumns {
    columns: &[column("secrets", "Secrets", 70.)],
    cells: |object| {
        typed_cells::<ServiceAccount>(object, |account| {
            vec![Cell::Number(
                account.secrets.as_ref().map_or(0, Vec::len) as i64
            )]
        })
    },
};

/// `ClusterRole/view`, as `kubectl` names a binding's role - or empty for a
/// binding without one, which `k8s-openapi` reads as an empty `RoleRef` rather
/// than failing.
fn role_text(role: &RoleRef) -> Cell {
    if role.name.is_empty() {
        return Cell::Empty;
    }
    Cell::text(format!("{}/{}", role.kind, role.name))
}

/// Each subject as `kind name` (`ServiceAccount ns/name` for one with a
/// namespace), comma-separated.
fn subjects_text(subjects: Option<&Vec<Subject>>) -> Cell {
    let subjects: Vec<String> = subjects
        .into_iter()
        .flatten()
        .map(|subject| match subject.namespace.as_deref() {
            Some(namespace) if subject.kind == "ServiceAccount" => {
                format!("ServiceAccount {namespace}/{}", subject.name)
            }
            _ => format!("{} {}", subject.kind, subject.name),
        })
        .collect();
    Cell::text(subjects.join(", "))
}

pub(super) static ROLE_BINDING: KindColumns = KindColumns {
    columns: &[ROLE, SUBJECTS],
    cells: |object| {
        typed_cells::<RoleBinding>(object, |binding| {
            vec![
                role_text(&binding.role_ref),
                subjects_text(binding.subjects.as_ref()),
            ]
        })
    },
};

pub(super) static CLUSTER_ROLE_BINDING: KindColumns = KindColumns {
    columns: &[ROLE, SUBJECTS],
    cells: |object| {
        typed_cells::<ClusterRoleBinding>(object, |binding| {
            vec![
                role_text(&binding.role_ref),
                subjects_text(binding.subjects.as_ref()),
            ]
        })
    },
};

#[cfg(test)]
mod tests;
