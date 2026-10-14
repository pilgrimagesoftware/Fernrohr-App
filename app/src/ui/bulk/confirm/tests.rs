//! Each bulk action's question, and which tier it confirms in.

use super::confirmation;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::bulk::{BulkAction, BulkTarget, MetadataMap, Mutation};
use crate::ui::confirm_dialog::Severity;
use kube::core::GroupVersionKind;

fn kind(group: &str, kind: &str, plural: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk(group, "v1", kind),
        plural: plural.into(),
        namespaced,
        verbs: Default::default(),
    }
}

fn targets(kind: &DiscoveredKind, count: usize) -> Vec<BulkTarget> {
    (0..count)
        .map(|n| BulkTarget {
            kind: kind.clone(),
            namespace: kind.namespaced.then(|| "shop".to_string()),
            name: format!("{}-{n}", kind.plural),
        })
        .collect()
}

fn title(action: BulkAction, targets: &[BulkTarget], mutation: Option<&Mutation>) -> String {
    confirmation(action, targets, mutation).title.to_string()
}

#[test]
fn each_action_names_how_many_of_which_kind() {
    let deployments = kind("apps", "Deployment", "deployments", true);
    let daemon_sets = kind("apps", "DaemonSet", "daemonsets", true);
    let nodes = kind("", "Node", "nodes", false);
    let pods = DiscoveredKind::pods();

    assert_eq!(
        title(BulkAction::Delete, &targets(&pods, 4), None),
        "Delete 4 Pods?"
    );
    let mut mixed = targets(&deployments, 2);
    mixed.extend(targets(&daemon_sets, 1));
    assert_eq!(
        title(BulkAction::RestartRollout, &mixed, None),
        "Restart Rollout for 2 Deployments, 1 DaemonSet?"
    );
    assert_eq!(
        title(
            BulkAction::Scale,
            &targets(&deployments, 2),
            Some(&Mutation::Scale { replicas: 3 })
        ),
        "Scale 2 Deployments to 3 replicas?"
    );
    assert_eq!(
        title(BulkAction::Cordon, &targets(&nodes, 2), None),
        "Cordon 2 Nodes?"
    );
    assert_eq!(
        title(BulkAction::Drain, &targets(&nodes, 2), None),
        "Drain 2 Nodes?"
    );
    assert_eq!(
        title(BulkAction::ViewLogs, &targets(&pods, 8), None),
        "Open 8 Logs panels?"
    );
}

#[test]
fn label_and_annotate_name_the_key_and_value_or_the_removal() {
    let mut mixed = targets(&DiscoveredKind::pods(), 1);
    mixed.extend(targets(&kind("", "ConfigMap", "configmaps", true), 1));
    let set = |map, value: Option<&str>| Mutation::SetMetadata {
        map,
        key: "team".into(),
        value: value.map(str::to_string),
    };
    assert_eq!(
        title(
            BulkAction::Label,
            &mixed,
            Some(&set(MetadataMap::Labels, Some("platform")))
        ),
        "Set label team=platform on 1 Pod, 1 ConfigMap?"
    );
    assert_eq!(
        title(
            BulkAction::Annotate,
            &mixed,
            Some(&set(MetadataMap::Annotations, None))
        ),
        "Remove annotation team from 1 Pod, 1 ConfigMap?"
    );
}

#[test]
fn delete_and_drain_are_irreversible_and_the_rest_recoverable() {
    let pods = targets(&DiscoveredKind::pods(), 1);
    for action in BulkAction::ALL {
        let expected = if matches!(action, BulkAction::Delete | BulkAction::Drain) {
            Severity::Irreversible
        } else {
            Severity::Recoverable
        };
        assert_eq!(
            confirmation(action, &pods, None).severity,
            expected,
            "{action:?}"
        );
    }
}

#[test]
fn a_long_set_lists_the_first_names_and_counts_the_rest() {
    let body = confirmation(
        BulkAction::Delete,
        &targets(&DiscoveredKind::pods(), 11),
        None,
    )
    .body;
    assert_eq!(body.name_list().len(), 8);
    assert!(body.plain().contains("and 3 more"), "{}", body.plain());
}
