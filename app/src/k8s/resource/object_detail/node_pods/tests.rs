//! `applies_to`'s own dispatch: the pure part of #186 that decides which
//! kind gets the region, exercised without a window or a cluster. The live
//! behaviour over a fake cluster is `object_detail::tests::node_pods`'s.

use super::applies_to;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::ui::nav::ObjectTarget;
use kube::core::GroupVersionKind;

fn target(group: &str, kind: &str, namespaced: bool, namespace: Option<&str>) -> ObjectTarget {
    ObjectTarget {
        kind: DiscoveredKind {
            gvk: GroupVersionKind::gvk(group, "v1", kind),
            plural: format!("{}s", kind.to_lowercase()),
            namespaced,
            verbs: Default::default(),
        },
        namespace: namespace.map(str::to_string),
        name: "node-a".into(),
    }
}

#[test]
fn only_the_core_cluster_scoped_node_kind_applies() {
    assert!(applies_to(&target("", "Node", false, None)));
}

#[test]
fn a_namespaced_kind_named_node_does_not_apply() {
    // Guards against matching on the bare name alone - a hypothetical CRD
    // also called "Node" in its own group must not grow this region.
    assert!(!applies_to(&target(
        "widgets.example.com",
        "Node",
        true,
        Some("staging")
    )));
}

#[test]
fn every_other_core_kind_does_not_apply() {
    assert!(!applies_to(&target("", "Namespace", false, None)));
    assert!(!applies_to(&target(
        "apps",
        "Deployment",
        true,
        Some("staging")
    )));
}
