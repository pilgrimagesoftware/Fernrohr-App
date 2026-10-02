//! Access-control kinds' sections - Role, ClusterRole, RoleBinding and
//! ClusterRoleBinding: what a role allows, and who a binding grants it to. A
//! binding's role and its ServiceAccount subjects are references.

use super::super::model::{FieldValue, ObjectField, ObjectSection};
use super::common::{non_empty, selector_chips};
use crate::k8s::object_ref::ObjectRef;
use k8s_openapi::api::rbac::v1::{
    ClusterRole, ClusterRoleBinding, PolicyRule, Role, RoleBinding, RoleRef, Subject,
};

/// The API group RBAC kinds live in.
const RBAC_GROUP: &str = "rbac.authorization.k8s.io";

/// `apps/deployments, apps/replicasets [names web]: get, list`. The core group
/// reads as `core`, and an empty list as `*` - every rule names at least one
/// verb, but may leave groups or resources to mean "all".
fn rule(rule: &PolicyRule) -> String {
    let groups: Vec<&str> = rule
        .api_groups
        .iter()
        .flatten()
        .map(|group| {
            if group.is_empty() {
                "core"
            } else {
                group.as_str()
            }
        })
        .collect();
    let groups = if groups.is_empty() { vec!["*"] } else { groups };
    let resources: Vec<&str> = rule
        .resources
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let targets: Vec<String> = if resources.is_empty() {
        rule.non_resource_urls.iter().flatten().cloned().collect()
    } else {
        groups
            .iter()
            .flat_map(|group| {
                resources
                    .iter()
                    .map(move |resource| format!("{group}/{resource}"))
            })
            .collect()
    };
    let names = rule
        .resource_names
        .as_deref()
        .filter(|names| !names.is_empty())
        .map(|names| format!(" [names {}]", names.join(", ")))
        .unwrap_or_default();
    format!(
        "{}{names}: {}",
        if targets.is_empty() {
            "*".to_string()
        } else {
            targets.join(", ")
        },
        rule.verbs.join(", ")
    )
}

fn rules(fields: &mut Vec<ObjectField>, rules: Option<&Vec<PolicyRule>>) {
    let lines: Vec<String> = rules.into_iter().flatten().map(rule).collect();
    fields.push(if lines.is_empty() {
        ObjectField::text("Rules", "none")
    } else {
        ObjectField::new("Rules", FieldValue::Lines(lines))
    });
}

pub(super) fn role(role: &Role) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    rules(&mut fields, role.rules.as_ref());
    vec![ObjectSection::new("Rules", fields)]
}

/// A ClusterRole may also be an aggregate: its rules are filled in from every
/// ClusterRole its selectors match, shown so the rules' origin isn't a mystery.
pub(super) fn cluster_role(role: &ClusterRole) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    rules(&mut fields, role.rules.as_ref());
    let aggregates: Vec<String> = role
        .aggregation_rule
        .iter()
        .flat_map(|rule| rule.cluster_role_selectors.iter().flatten())
        .flat_map(selector_chips)
        .collect();
    if !aggregates.is_empty() {
        fields.push(ObjectField::new(
            "Aggregates",
            FieldValue::Chips(aggregates),
        ));
    }
    vec![ObjectSection::new("Rules", fields)]
}

/// The bound role: a Role in the binding's own namespace, or a ClusterRole.
fn role_ref(role: &RoleRef, namespace: Option<&str>) -> ObjectRef {
    match (role.kind.as_str(), namespace) {
        ("Role", Some(namespace)) => {
            ObjectRef::namespaced(RBAC_GROUP, "Role", namespace, &role.name)
        }
        (kind, _) => ObjectRef::cluster_scoped(RBAC_GROUP, kind, &role.name),
    }
}

/// `ServiceAccount staging/deployer`, `User alice`, `Group system:masters`.
fn subject(subject: &Subject, namespace: Option<&str>) -> String {
    let in_namespace = non_empty(subject.namespace.as_deref()).or(namespace);
    match (subject.kind.as_str(), in_namespace) {
        ("ServiceAccount", Some(namespace)) => {
            format!("ServiceAccount {namespace}/{}", subject.name)
        }
        (kind, _) => format!("{kind} {}", subject.name),
    }
}

/// A ServiceAccount subject as a reference - Users and Groups aren't objects.
fn service_account(subject: &Subject, namespace: Option<&str>) -> Option<ObjectRef> {
    if subject.kind != "ServiceAccount" {
        return None;
    }
    let namespace = non_empty(subject.namespace.as_deref()).or(namespace)?;
    Some(ObjectRef::core("ServiceAccount", namespace, &subject.name))
}

fn binding(
    role: &RoleRef,
    subjects: Option<&Vec<Subject>>,
    namespace: Option<&str>,
) -> Vec<ObjectSection> {
    let mut fields = vec![ObjectField::references(
        "Role",
        vec![role_ref(role, namespace)],
        true,
    )];
    let subjects: Vec<&Subject> = subjects.into_iter().flatten().collect();
    let lines: Vec<String> = subjects
        .iter()
        .map(|entry| subject(entry, namespace))
        .collect();
    if !lines.is_empty() {
        fields.push(ObjectField::new("Subjects", FieldValue::Lines(lines)));
    }
    let accounts: Vec<ObjectRef> = subjects
        .iter()
        .filter_map(|entry| service_account(entry, namespace))
        .collect();
    if !accounts.is_empty() {
        fields.push(ObjectField::references("Service Accounts", accounts, false));
    }
    vec![ObjectSection::new("Binding", fields)]
}

pub(super) fn role_binding(role_binding: &RoleBinding, namespace: &str) -> Vec<ObjectSection> {
    binding(
        &role_binding.role_ref,
        role_binding.subjects.as_ref(),
        Some(namespace),
    )
}

/// A ClusterRoleBinding has no namespace of its own: a ServiceAccount subject
/// must name its namespace to be a reference.
pub(super) fn cluster_role_binding(cluster_binding: &ClusterRoleBinding) -> Vec<ObjectSection> {
    binding(
        &cluster_binding.role_ref,
        cluster_binding.subjects.as_ref(),
        None,
    )
}
