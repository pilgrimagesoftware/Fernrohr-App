//! Access-control kinds' sections (`standard-resource-panels` 3.4): Role,
//! ClusterRole, RoleBinding and ClusterRoleBinding.

use super::fixtures::{kind, object};
use super::sections::field;
use crate::k8s::object_ref::ObjectRef;
use crate::k8s::resource::object_detail::model::FieldValue;
use crate::k8s::resource::object_detail::sections::sections_for;
use serde_json::json;

const RBAC: &str = "rbac.authorization.k8s.io";

/// Each rule's API groups, resources, resource names and verbs - the core
/// group as `core`, and resources crossed with their groups.
#[test]
fn a_role_shows_each_rule() {
    let role = object(json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "Role",
        "metadata": { "name": "deployer", "namespace": "staging" },
        "rules": [
            { "apiGroups": [""], "resources": ["pods", "pods/log"], "verbs": ["get", "list"] },
            { "apiGroups": ["apps"], "resources": ["deployments"],
              "resourceNames": ["web"], "verbs": ["patch"] },
        ],
    }));

    let sections = sections_for(&kind(RBAC, "v1", "Role", true), &role);

    assert_eq!(sections[0].title, "Rules");
    assert_eq!(
        field(&sections, "Rules").value,
        FieldValue::Lines(vec![
            "core/pods, core/pods/log: get, list".into(),
            "apps/deployments [names web]: patch".into(),
        ])
    );
}

/// A ClusterRole's rules can name non-resource URLs, and an aggregate
/// ClusterRole shows the selectors its rules are gathered from.
#[test]
fn a_cluster_role_shows_url_rules_and_its_aggregation() {
    let role = object(json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "ClusterRole",
        "metadata": { "name": "monitoring" },
        "rules": [
            { "nonResourceURLs": ["/healthz", "/metrics"], "verbs": ["get"] },
            { "apiGroups": ["*"], "resources": ["*"], "verbs": ["list"] },
        ],
        "aggregationRule": {
            "clusterRoleSelectors": [{ "matchLabels": { "rbac.example.com/aggregate-to-monitoring": "true" } }],
        },
    }));

    let sections = sections_for(&kind(RBAC, "v1", "ClusterRole", false), &role);

    assert_eq!(
        field(&sections, "Rules").value,
        FieldValue::Lines(vec!["/healthz, /metrics: get".into(), "*/*: list".into(),])
    );
    assert_eq!(
        field(&sections, "Aggregates").value.text(),
        "rbac.example.com/aggregate-to-monitoring=true"
    );
}

/// A RoleBinding's Role is in its own namespace; each subject is listed, and
/// each ServiceAccount subject is a reference - in the binding's namespace
/// unless it names its own. Users and Groups aren't objects to follow.
#[test]
fn a_role_binding_references_its_role_and_service_account_subjects() {
    let binding = object(json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "RoleBinding",
        "metadata": { "name": "deployers", "namespace": "staging" },
        "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "Role", "name": "deployer" },
        "subjects": [
            { "kind": "ServiceAccount", "name": "ci" },
            { "kind": "ServiceAccount", "name": "argo", "namespace": "argocd" },
            { "kind": "User", "name": "alice", "apiGroup": "rbac.authorization.k8s.io" },
            { "kind": "Group", "name": "devs", "apiGroup": "rbac.authorization.k8s.io" },
        ],
    }));

    let sections = sections_for(&kind(RBAC, "v1", "RoleBinding", true), &binding);

    assert_eq!(sections[0].title, "Binding");
    assert_eq!(
        field(&sections, "Role").value,
        FieldValue::References {
            targets: vec![ObjectRef::namespaced(RBAC, "Role", "staging", "deployer")],
            qualified: true,
        }
    );
    assert_eq!(
        field(&sections, "Subjects").value,
        FieldValue::Lines(vec![
            "ServiceAccount staging/ci".into(),
            "ServiceAccount argocd/argo".into(),
            "User alice".into(),
            "Group devs".into(),
        ])
    );
    assert_eq!(
        field(&sections, "Service Accounts").value,
        FieldValue::References {
            targets: vec![
                ObjectRef::core("ServiceAccount", "staging", "ci"),
                ObjectRef::core("ServiceAccount", "argocd", "argo"),
            ],
            qualified: false,
        }
    );
}

/// A RoleBinding may bind a ClusterRole, which is cluster-scoped wherever the
/// binding is.
#[test]
fn a_role_binding_to_a_cluster_role_references_it_cluster_scoped() {
    let binding = object(json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "RoleBinding",
        "metadata": { "name": "viewers", "namespace": "staging" },
        "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": "view" },
        "subjects": [{ "kind": "Group", "name": "auditors", "apiGroup": "rbac.authorization.k8s.io" }],
    }));

    let sections = sections_for(&kind(RBAC, "v1", "RoleBinding", true), &binding);

    assert_eq!(
        field(&sections, "Role").value,
        FieldValue::References {
            targets: vec![ObjectRef::cluster_scoped(RBAC, "ClusterRole", "view")],
            qualified: true,
        }
    );
}

#[test]
fn a_cluster_role_binding_references_its_role_and_service_account_subjects() {
    let binding = object(json!({
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "ClusterRoleBinding",
        "metadata": { "name": "cluster-admins" },
        "roleRef": { "apiGroup": "rbac.authorization.k8s.io", "kind": "ClusterRole", "name": "cluster-admin" },
        "subjects": [
            { "kind": "ServiceAccount", "name": "admin", "namespace": "kube-system" },
            { "kind": "Group", "name": "system:masters", "apiGroup": "rbac.authorization.k8s.io" },
        ],
    }));

    let sections = sections_for(&kind(RBAC, "v1", "ClusterRoleBinding", false), &binding);

    assert_eq!(
        field(&sections, "Role").value.text(),
        "ClusterRole/cluster-admin"
    );
    assert_eq!(
        field(&sections, "Subjects").value.text(),
        "ServiceAccount kube-system/admin, Group system:masters"
    );
    assert_eq!(
        field(&sections, "Service Accounts").value,
        FieldValue::References {
            targets: vec![ObjectRef::core("ServiceAccount", "kube-system", "admin")],
            qualified: false,
        }
    );
}
