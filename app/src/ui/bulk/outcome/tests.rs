//! A batch's outcome summed per checked object.

use super::BatchOutcome;
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::bulk::{BulkTarget, ObjectOutcome};
use crate::k8s::resource::resource_actions::ActionFailure;

fn outcome(name: &str, part: Option<&str>, ok: bool) -> ObjectOutcome {
    ObjectOutcome {
        target: BulkTarget {
            kind: DiscoveredKind::pods(),
            namespace: Some("shop".into()),
            name: name.into(),
        },
        part: part.map(str::to_string),
        result: if ok {
            Ok(())
        } else {
            Err(ActionFailure {
                message: "Already deleted.".into(),
                detail: "pods \"web-5\" not found".into(),
            })
        },
    }
}

#[test]
fn one_failure_in_five_names_the_one_and_counts_the_rest() {
    let outcomes: Vec<ObjectOutcome> = (1..=5)
        .map(|n| outcome(&format!("web-{n}"), None, n != 5))
        .collect();
    let batch = BatchOutcome::of("Delete", &outcomes);
    assert_eq!(batch.summary(), "Delete: 4 succeeded, 1 failed");
    assert_eq!(batch.failed.len(), 1);
    assert_eq!(batch.failed[0].name, "shop/web-5");
    assert_eq!(batch.failed[0].failure.message, "Already deleted.");
}

#[test]
fn an_object_with_any_failed_part_did_not_succeed() {
    let outcomes = [
        outcome("node-a", None, true),
        outcome("node-a", Some("evict shop/web-2"), false),
        outcome("node-b", None, true),
    ];
    let batch = BatchOutcome::of("Drain", &outcomes);
    let succeeded: Vec<&str> = batch
        .succeeded
        .iter()
        .map(|key| key.name.as_str())
        .collect();
    assert_eq!(succeeded, ["node-b"]);
    assert_eq!(batch.failed[0].name, "shop/node-a: evict shop/web-2");
}
