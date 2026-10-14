//! Which actions a checked set is offered, and how it is named (tasks 2.1, 3.1).

use super::*;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::bulk::{BulkAction, count_of_kinds};

fn offered(kinds: &[DiscoveredKind]) -> Vec<BulkAction> {
    BulkAction::offered(kinds)
}

#[test]
fn restart_and_rollback_need_every_object_to_be_a_rollout_kind() {
    let mixed = offered(&[deployment(), stateful_set(), daemon_set()]);
    assert!(mixed.contains(&BulkAction::RestartRollout), "{mixed:?}");
    assert!(mixed.contains(&BulkAction::RollbackRollout), "{mixed:?}");

    let with_service = offered(&[deployment(), service()]);
    assert!(!with_service.contains(&BulkAction::RestartRollout));
    assert!(!with_service.contains(&BulkAction::RollbackRollout));
    assert!(!offered(&[replica_set()]).contains(&BulkAction::RestartRollout));
}

#[test]
fn scale_needs_every_object_to_be_scalable() {
    assert!(offered(&[deployment(), replica_set(), stateful_set()]).contains(&BulkAction::Scale));
    assert!(!offered(&[deployment(), DiscoveredKind::pods()]).contains(&BulkAction::Scale));
    assert!(!offered(&[daemon_set()]).contains(&BulkAction::Scale));
}

#[test]
fn cordon_uncordon_and_drain_are_for_nodes_alone() {
    let nodes = offered(&[node(), node()]);
    for action in [BulkAction::Cordon, BulkAction::Uncordon, BulkAction::Drain] {
        assert!(nodes.contains(&action), "{action:?} for nodes");
        assert!(
            !offered(&[node(), DiscoveredKind::pods()]).contains(&action),
            "{action:?} with a pod checked too"
        );
    }
}

#[test]
fn view_logs_is_for_pods_alone() {
    assert!(offered(&[DiscoveredKind::pods()]).contains(&BulkAction::ViewLogs));
    assert!(!offered(&[deployment()]).contains(&BulkAction::ViewLogs));
}

#[test]
fn any_mix_still_offers_delete_label_annotate_and_the_copies() {
    let mixed = offered(&[DiscoveredKind::pods(), config_map(), service()]);
    assert_eq!(
        mixed,
        [
            BulkAction::Label,
            BulkAction::Annotate,
            BulkAction::CopyNames,
            BulkAction::CopyYaml,
            BulkAction::Delete,
        ]
    );
}

#[test]
fn a_kind_that_cant_be_patched_or_deleted_isnt_offered_them() {
    let mut read_only = deployment();
    read_only.verbs.patch = false;
    read_only.verbs.delete = false;
    assert_eq!(
        offered(&[read_only]),
        [BulkAction::CopyNames, BulkAction::CopyYaml]
    );
}

#[test]
fn nothing_is_offered_with_nothing_checked() {
    assert!(offered(&[]).is_empty());
}

#[test]
fn a_checked_set_is_named_by_how_many_of_each_kind() {
    let kinds = [deployment(), deployment(), daemon_set()];
    assert_eq!(count_of_kinds(&kinds), "2 Deployments, 1 DaemonSet");
    assert_eq!(
        count_of_kinds(&[DiscoveredKind::pods(), config_map()]),
        "1 Pod, 1 ConfigMap"
    );
    assert_eq!(count_of_kinds(&[node(), node()]), "2 Nodes");
}
