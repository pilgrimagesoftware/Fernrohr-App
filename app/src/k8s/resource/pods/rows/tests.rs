// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use super::matches_node;
use crate::k8s::resource::pods::{matches_namespaces, pod_row};
use jiff::Timestamp;
use k8s_openapi::api::core::v1::{Pod, PodSpec};

use crate::k8s::resource::pods::test_support::*;

#[test]
fn pod_row_reports_ready_status_restarts_and_age() {
    let now = Timestamp::from_second(90).unwrap();
    let row = pod_row(&pod("u1", "web-1"), now);

    assert_eq!(row.name, "web-1");
    assert_eq!(row.namespace, "default");
    assert_eq!(row.ready, "1/1");
    assert_eq!(row.status, "Running");
    assert_eq!(row.restarts, 2);
    assert_eq!(row.age, "1m");
}

/// #120: a pod that ran to completion - Succeeded, its container exited 0 and
/// so not ready - reads 0/1 in a neutral tone, not the not-ready warning.
#[test]
fn a_succeeded_pods_ready_count_is_no_warning() {
    let mut succeeded = pod("u1", "job-1");
    let status = succeeded.status.as_mut().expect("the fixture has a status");
    status.phase = Some("Succeeded".into());
    for container in status.container_statuses.iter_mut().flatten() {
        container.ready = false;
    }
    let row = pod_row(&succeeded, Timestamp::from_second(90).unwrap());
    assert_eq!(row.ready, "0/1");
    assert_eq!(row.ready_tone, crate::ui::style::Tone::Neutral);
}

#[test]
fn multiple_namespace_scope_includes_each_selected_namespace() {
    let pods = mixed_namespace_fixture();
    let namespaces = vec!["default".to_string(), "kube-system".to_string()];
    let selected: Vec<&Pod> = pods
        .iter()
        .filter(|pod| matches_namespaces(pod, &namespaces))
        .collect();
    assert_eq!(selected.len(), 3);
    assert!(!matches_namespaces(
        &pod_in("other", "u4", "ignored", 0),
        &namespaces
    ));
}

/// #186: a Node's embedded table keeps only the pods `spec.nodeName` names,
/// across every namespace; the standalone Pods panel's own table (`node:
/// None`) keeps every pod regardless of which node it's on.
#[test]
fn matches_node_scopes_to_one_node_across_namespaces() {
    let on_a = on_node("default", "u1", "web-1", "node-a");
    let also_on_a = on_node("kube-system", "u2", "coredns-1", "node-a");
    let on_b = on_node("default", "u3", "web-2", "node-b");

    assert!(matches_node(&on_a, Some("node-a")));
    assert!(matches_node(&also_on_a, Some("node-a")));
    assert!(!matches_node(&on_b, Some("node-a")));
    assert!(matches_node(&on_a, None), "no node scope keeps every pod");
}

fn on_node(namespace: &str, uid: &str, name: &str, node: &str) -> Pod {
    Pod {
        spec: Some(PodSpec {
            node_name: Some(node.into()),
            ..Default::default()
        }),
        ..pod_in(namespace, uid, name, 0)
    }
}

mod tones {
    //! The projection's status and readiness tones - what the Pods table's
    //! Status and Ready cells are coloured by.
    use crate::k8s::resource::pods::rows::status_tone;
    use crate::ui::style::Tone;
    use k8s_openapi::api::core::v1::{
        ContainerState, ContainerStateWaiting, ContainerStatus, PodStatus,
    };

    fn phase(phase: &str) -> PodStatus {
        PodStatus {
            phase: Some(phase.into()),
            ..Default::default()
        }
    }

    fn waiting(reason: &str) -> ContainerStatus {
        ContainerStatus {
            name: "app".into(),
            state: Some(ContainerState {
                waiting: Some(ContainerStateWaiting {
                    reason: Some(reason.into()),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn each_phase_has_its_tone() {
        assert_eq!(status_tone(Some(&phase("Running"))), Tone::Good);
        assert_eq!(status_tone(Some(&phase("Pending"))), Tone::Warning);
        assert_eq!(status_tone(Some(&phase("Failed"))), Tone::Bad);
        assert_eq!(status_tone(Some(&phase("Succeeded"))), Tone::Neutral);
        assert_eq!(status_tone(Some(&phase("Unknown"))), Tone::Neutral);
        assert_eq!(status_tone(None), Tone::Neutral);
    }

    /// A running pod whose container is crash-looping, or a pending one that
    /// can't pull its image, is bad whatever its phase says.
    #[test]
    fn a_stuck_container_is_bad() {
        for (pod_phase, reason) in [
            ("Running", "CrashLoopBackOff"),
            ("Pending", "ImagePullBackOff"),
            ("Pending", "ErrImagePull"),
        ] {
            let status = PodStatus {
                container_statuses: Some(vec![waiting(reason)]),
                ..phase(pod_phase)
            };
            assert_eq!(
                status_tone(Some(&status)),
                Tone::Bad,
                "{pod_phase} + {reason}"
            );
        }
    }

    #[test]
    fn a_stuck_init_container_is_bad_too() {
        let status = PodStatus {
            init_container_statuses: Some(vec![waiting("CrashLoopBackOff")]),
            ..phase("Pending")
        };
        assert_eq!(status_tone(Some(&status)), Tone::Bad);
    }

    /// Waiting on something ordinary - a container still being created - is
    /// just pending.
    #[test]
    fn an_ordinary_wait_keeps_the_phase_tone() {
        let status = PodStatus {
            container_statuses: Some(vec![waiting("ContainerCreating")]),
            ..phase("Pending")
        };
        assert_eq!(status_tone(Some(&status)), Tone::Warning);
    }

    #[test]
    fn readiness_is_good_when_all_ready_and_a_warning_otherwise() {
        use super::super::ready_tone;
        assert_eq!(ready_tone(2, 2, Some("Running")), Tone::Good);
        assert_eq!(ready_tone(1, 2, Some("Running")), Tone::Warning);
        assert_eq!(ready_tone(0, 0, None), Tone::Neutral);
        assert_eq!(
            ready_tone(0, 1, Some("Succeeded")),
            Tone::Neutral,
            "a pod that ran to completion is no warning (#120)"
        );
        assert_eq!(
            ready_tone(0, 1, Some("Failed")),
            Tone::Warning,
            "a failed one still is"
        );
    }
}

/// #121: the restart count's colour - red while a restart is recent, orange
/// past the many-restarts threshold, yellow for any, neutral for none.
#[test]
fn restart_tone_ranks_recent_over_many_over_any() {
    use crate::k8s::resource::pods::rows::restart_tone;
    use crate::ui::style::Tone;
    use jiff::SignedDuration;
    let minutes = |m: i64| Some(SignedDuration::from_mins(m));

    assert_eq!(restart_tone(0, None), Tone::Neutral);
    assert_eq!(restart_tone(1, None), Tone::Warning);
    assert_eq!(restart_tone(10, minutes(60)), Tone::Warning);
    assert_eq!(restart_tone(11, minutes(60)), Tone::Serious);
    assert_eq!(restart_tone(1, minutes(14)), Tone::Bad);
    assert_eq!(restart_tone(50, minutes(14)), Tone::Bad);
    assert_eq!(restart_tone(1, minutes(15)), Tone::Warning);
    // A finish just ahead of the local clock (skew) is as recent as it gets.
    assert_eq!(restart_tone(1, minutes(-1)), Tone::Bad);
}

/// #121: the row reads the latest `lastState.terminated.finishedAt` across its
/// containers, so one container's fresh restart turns the whole count red.
#[test]
fn a_pod_restarted_minutes_ago_reads_bad() {
    use crate::ui::style::Tone;
    use k8s_openapi::api::core::v1::{ContainerState, ContainerStateTerminated, ContainerStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;

    let restarted_at = |secs: i64| ContainerState {
        terminated: Some(ContainerStateTerminated {
            finished_at: Some(Time(Timestamp::from_second(secs).unwrap())),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut pod = pod("u1", "web-1");
    let statuses = pod
        .status
        .as_mut()
        .and_then(|status| status.container_statuses.as_mut())
        .expect("the fixture has container statuses");
    statuses[0].last_state = Some(restarted_at(0));
    statuses.push(ContainerStatus {
        name: "sidecar".into(),
        restart_count: 1,
        last_state: Some(restarted_at(3_000)),
        ..Default::default()
    });

    let soon_after = Timestamp::from_second(3_000 + 5 * 60).unwrap();
    assert_eq!(pod_row(&pod, soon_after).restart_tone, Tone::Bad);
    let long_after = Timestamp::from_second(3_000 + 60 * 60).unwrap();
    assert_eq!(pod_row(&pod, long_after).restart_tone, Tone::Warning);
}
