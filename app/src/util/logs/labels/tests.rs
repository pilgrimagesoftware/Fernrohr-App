//! Which containers a label selector streams (#150), and that a panel keeps
//! its streams in step with the pods it matches.

use super::render::counts_text;
use super::{
    LabelLogs, Matched, SavedLabels, TypedId, WorkloadRef, container_names, labels_from_state,
    wanted,
};
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
    let (wanted, matched) = wanted(pods, &parse(selector).unwrap(), &namespaces, None);
    (
        wanted
            .into_iter()
            .map(|want| want.source.tag().to_string())
            .collect(),
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
    let (before, _) = wanted(&before, &selector, &[], None);
    let (after, _) = wanted(&after, &selector, &[], None);
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
        Some(SavedLabels {
            source: workload,
            selector: Some("app=web".to_string()),
            container: None,
        })
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
    let (before, _) = wanted(&before, &selector, &[], None);
    let (after, _) = wanted(&recreated, &selector, &[], None);
    assert_eq!(before[0].source, after[0].source, "the same source");
    assert_ne!(before[0].key, after[0].key);
}

/// `selector-logs` "More matches than the cap": the bar says how many matched
/// and, past the cap, that only that many stream.
#[test]
fn the_bar_says_when_only_the_cap_streams() {
    let matched = |pods, containers| Matched { pods, containers };
    assert_eq!(counts_text(matched(1, 1)), "1 pod · 1 container");
    assert_eq!(counts_text(matched(20, 20)), "20 pods · 20 containers");
    assert_eq!(
        counts_text(matched(25, 25)),
        "25 pods · 25 containers · streaming 20"
    );
}

/// `selector-logs-panel` 4.1: the container filter streams only the named
/// container from each pod, tagged with the pod alone, and leaves the rest out
/// of the count; the names it offers are every matched pod's.
#[test]
fn the_container_filter_streams_one_container_per_pod() {
    let pods = vec![
        pod(
            "shop",
            "web-1",
            &[("app", "web")],
            vec![
                container("app", running(), 0),
                container("sidecar", running(), 0),
            ],
        ),
        pod(
            "shop",
            "web-2",
            &[("app", "web")],
            vec![container("app", running(), 0)],
        ),
    ];
    let selector = parse("app=web").unwrap();
    let tags = |filter: Option<&str>| {
        let (wanted, matched) = wanted(&pods, &selector, &[], filter);
        let tags: Vec<String> = wanted
            .iter()
            .map(|want| want.source.tag().to_string())
            .collect();
        (tags, matched.containers)
    };
    assert_eq!(
        tags(None),
        (
            vec!["web-1/app".into(), "web-1/sidecar".into(), "web-2".into()],
            3
        )
    );
    assert_eq!(tags(Some("sidecar")), (vec!["web-1".into()], 1));
    assert_eq!(tags(Some("app")), (vec!["web-1".into(), "web-2".into()], 2));
    assert_eq!(container_names(&pods, &selector, &[]), ["app", "sidecar"]);
}

/// 4.2: a saved panel's container filter comes back with it.
#[test]
fn a_saved_panel_keeps_its_container_filter() {
    let state = serde_json::json!({
        "context_name": "dev",
        "label_logs": serde_json::to_value(LabelLogs::Typed(TypedId::LEGACY)).unwrap(),
        "selector": "app=web",
        "container_filter": "sidecar",
    });
    let saved = labels_from_state(&state).expect("a label panel");
    assert_eq!(saved.container.as_deref(), Some("sidecar"));
}

/// #238 review: a stream first started reads its last lines; one the view
/// showed before and dropped resumes from the second after it stopped, with
/// no tail, so the lines it already showed aren't added again.
#[test]
fn a_resumed_stream_starts_where_it_stopped() {
    use super::StreamKey;
    use super::follow::label_target;
    use crate::consts::LABEL_LOGS_TAIL_LINES;
    let key = StreamKey {
        namespace: "shop".into(),
        pod: "web-1".into(),
        uid: "uid-web-1".into(),
        container: "app".into(),
        restart_count: 0,
    };
    let first = label_target(&key, "dev", true, None);
    assert_eq!(first.tail_lines, Some(LABEL_LOGS_TAIL_LINES));
    assert_eq!(first.since, None);
    assert!(first.follow);

    let stopped: jiff::Timestamp = "2026-10-12T18:00:00.400Z".parse().unwrap();
    let resumed = label_target(&key, "dev", false, Some(stopped));
    assert_eq!(resumed.tail_lines, None, "no tail replayed");
    assert_eq!(
        resumed.since,
        Some("2026-10-12T18:00:01Z".parse().unwrap()),
        "from the next whole second"
    );
    assert!(!resumed.follow, "a terminated container is read to its end");
}

/// #238 review, optional: `c` steps all → each name → all, and a filter naming
/// a container the pods don't have steps back to all.
#[test]
fn the_container_filter_steps_through_all_then_each_name() {
    use super::follow::next_filter;
    let names = ["app".to_string(), "sidecar".to_string()];
    assert_eq!(next_filter(None, &names).as_deref(), Some("app"));
    assert_eq!(next_filter(Some("app"), &names).as_deref(), Some("sidecar"));
    assert_eq!(next_filter(Some("sidecar"), &names), None);
    assert_eq!(next_filter(Some("gone"), &names), None, "back to all");
    assert_eq!(next_filter(None, &[]), None);
}

/// #238 review: a filter no matching pod's container has says so, rather than
/// that the containers haven't started.
#[test]
fn a_filter_naming_no_container_says_so() {
    use super::Following;
    use super::render::no_lines_message;
    let mut following = Following::new(
        LabelLogs::new_typed(),
        Some("app=web"),
        Some("sidecar".into()),
    );
    following.matched = Matched {
        pods: 2,
        containers: 0,
    };
    following.containers = vec!["app".into()];
    assert_eq!(
        no_lines_message(&following, "app=web"),
        "No matching pod has a container named sidecar."
    );

    following.containers.push("sidecar".into());
    assert_eq!(
        no_lines_message(&following, "app=web"),
        "The matching pods' containers haven't started yet.",
        "named, just not started"
    );
    following.matched.containers = 1;
    assert_eq!(
        no_lines_message(&following, "app=web"),
        "Waiting for log lines…"
    );
}

/// `selector-logs-multiple-panels` 1.1: every new typed panel has an identity
/// of its own, and none is the legacy one.
#[test]
fn new_typed_panels_have_distinct_identities() {
    let ids: std::collections::HashSet<TypedId> = (0..64).map(|_| TypedId::new()).collect();
    assert_eq!(ids.len(), 64, "no two alike");
    assert!(!ids.contains(&TypedId::LEGACY));
    assert_ne!(LabelLogs::new_typed(), LabelLogs::new_typed());
    assert!(LabelLogs::new_typed().is_typed());
}

/// 1.2: `label_logs` reads today's form, a workload's, and the bare `"Typed"`
/// a layout saved before identities holds - as the fixed legacy one.
#[test]
fn label_logs_reads_current_and_legacy_forms() {
    let typed = LabelLogs::new_typed();
    let round = serde_json::from_value::<LabelLogs>(serde_json::to_value(&typed).unwrap());
    assert_eq!(round.unwrap(), typed);
    let workload = LabelLogs::Workload(WorkloadRef {
        kind: "Deployment".into(),
        namespace: "shop".into(),
        name: "web".into(),
        selector: "app=web".into(),
    });
    let round = serde_json::from_value::<LabelLogs>(serde_json::to_value(&workload).unwrap());
    assert_eq!(round.unwrap(), workload);
    assert_eq!(
        serde_json::from_value::<LabelLogs>(serde_json::json!("Typed")).unwrap(),
        LabelLogs::Typed(TypedId::LEGACY)
    );
    let legacy = serde_json::json!({
        "context_name": "dev", "label_logs": "Typed", "selector": "app=web",
    });
    assert_eq!(
        labels_from_state(&legacy).map(|saved| saved.source),
        Some(LabelLogs::Typed(TypedId::LEGACY))
    );
}
