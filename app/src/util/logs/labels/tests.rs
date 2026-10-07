//! Which containers a label selector streams (#150), and that a panel keeps
//! its streams in step with the pods it matches.

use super::{LabelLogs, Matched, WorkloadRef, labels_from_state, wanted};
use crate::k8s::label_selector::parse;
use k8s_openapi::api::core::v1::{
    ContainerState, ContainerStateRunning, ContainerStateTerminated, ContainerStateWaiting,
    ContainerStatus, Pod, PodStatus,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

pub(super) fn container(name: &str, state: ContainerState, restarts: i32) -> ContainerStatus {
    ContainerStatus {
        name: name.into(),
        state: Some(state),
        restart_count: restarts,
        ..Default::default()
    }
}

pub(super) fn running() -> ContainerState {
    ContainerState {
        running: Some(ContainerStateRunning::default()),
        ..Default::default()
    }
}

fn waiting() -> ContainerState {
    ContainerState {
        waiting: Some(ContainerStateWaiting::default()),
        ..Default::default()
    }
}

fn terminated() -> ContainerState {
    ContainerState {
        terminated: Some(ContainerStateTerminated::default()),
        ..Default::default()
    }
}

pub(super) fn pod(
    namespace: &str,
    name: &str,
    labels: &[(&str, &str)],
    containers: Vec<ContainerStatus>,
) -> Pod {
    Pod {
        metadata: ObjectMeta {
            name: Some(name.into()),
            namespace: Some(namespace.into()),
            uid: Some(format!("uid-{namespace}-{name}")),
            labels: Some(
                labels
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
            ),
            ..Default::default()
        },
        status: Some(PodStatus {
            phase: Some("Running".into()),
            container_statuses: Some(containers),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn sources(pods: &[Pod], selector: &str, namespaces: &[&str]) -> (Vec<String>, Matched) {
    let namespaces: Vec<String> = namespaces.iter().map(|ns| ns.to_string()).collect();
    let (wanted, matched) = wanted(pods, &parse(selector).unwrap(), &namespaces);
    (
        wanted.into_iter().map(|want| want.source).collect(),
        matched,
    )
}

#[test]
fn a_selector_streams_its_pods_started_containers_in_its_namespaces() {
    let pods = vec![
        pod(
            "shop",
            "web-2",
            &[("app", "web")],
            vec![container("app", running(), 0)],
        ),
        pod(
            "shop",
            "web-1",
            &[("app", "web")],
            vec![container("app", running(), 3)],
        ),
        pod(
            "shop",
            "db-1",
            &[("app", "db")],
            vec![container("db", running(), 0)],
        ),
        pod(
            "blog",
            "web-1",
            &[("app", "web")],
            vec![container("app", running(), 0)],
        ),
        pod(
            "shop",
            "web-3",
            &[("app", "web")],
            vec![container("app", waiting(), 0)],
        ),
    ];
    let (in_shop, matched) = sources(&pods, "app=web", &["shop"]);
    assert_eq!(in_shop, ["web-1", "web-2"], "sorted, started only");
    assert_eq!(
        matched,
        Matched {
            pods: 3,
            containers: 2
        }
    );

    let (everywhere, _) = sources(&pods, "app=web", &[]);
    assert_eq!(everywhere, ["web-1", "web-1", "web-2"]);
}

#[test]
fn a_pod_with_several_containers_tags_each_lines_with_the_container() {
    let pods = vec![pod(
        "shop",
        "web-1",
        &[("app", "web")],
        vec![
            container("app", running(), 0),
            container("sidecar", terminated(), 0),
        ],
    )];
    assert_eq!(
        sources(&pods, "app=web", &[]).0,
        ["web-1/app", "web-1/sidecar"]
    );
}

#[test]
fn a_restarted_container_is_a_new_stream() {
    let before = vec![pod(
        "shop",
        "web-1",
        &[("app", "web")],
        vec![container("app", running(), 0)],
    )];
    let after = vec![pod(
        "shop",
        "web-1",
        &[("app", "web")],
        vec![container("app", running(), 1)],
    )];
    let selector = parse("app=web").unwrap();
    let (before, _) = wanted(&before, &selector, &[]);
    let (after, _) = wanted(&after, &selector, &[]);
    assert_ne!(before[0].key, after[0].key);
}

#[test]
fn a_saved_panel_names_what_it_followed() {
    let workload = LabelLogs::Workload(WorkloadRef {
        kind: "Deployment".into(),
        namespace: "shop".into(),
        name: "web".into(),
        selector: "app=web".into(),
    });
    let state = serde_json::json!({
        "context_name": "dev",
        "label_logs": serde_json::to_value(&workload).unwrap(),
        "selector": "app=web",
    });
    assert_eq!(
        labels_from_state(&state),
        Some((workload, Some("app=web".to_string())))
    );
    assert_eq!(
        labels_from_state(&serde_json::json!({ "pod_name": "web-1" })),
        None
    );
}

/// Review of #167: a pod deleted and recreated under the same name - a
/// StatefulSet's, its restart count 0 again - is a new stream, not the old
/// pod's finished one.
#[test]
fn a_pod_recreated_under_the_same_name_is_a_new_stream() {
    let selector = parse("app=db").unwrap();
    let before = vec![pod(
        "shop",
        "db-0",
        &[("app", "db")],
        vec![container("db", running(), 0)],
    )];
    let mut recreated = before.clone();
    recreated[0].metadata.uid = Some("uid-recreated".into());
    let (before, _) = wanted(&before, &selector, &[]);
    let (after, _) = wanted(&recreated, &selector, &[]);
    assert_eq!(before[0].source, after[0].source, "the same tag");
    assert_ne!(before[0].key, after[0].key);
}
