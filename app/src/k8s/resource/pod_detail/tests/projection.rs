//! The `Pod` -> field-list projection.

use super::fixtures::{field, rich_pod};
use crate::k8s::resource::pod_detail::fields::pod_fields;
use crate::k8s::resource::pod_detail::model::{BadgeTone, DetailSection, PodFieldValue};
use jiff::Timestamp;
use k8s_openapi::api::core::v1::{Container, Pod, PodCondition, Toleration};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

/// Section 3.1: every field the design lists is present, in order, and a
/// pod with more than one label/annotation/toleration/condition keeps them
/// all as discrete items rather than collapsing them into one string.
#[test]
fn every_structured_field_is_projected_in_order() {
    let fields = pod_fields(&rich_pod(), Timestamp::from_second(90).unwrap());

    let labels: Vec<&str> = fields.iter().map(|f| f.label).collect();
    assert_eq!(
        labels,
        vec![
            "Created",
            "Name",
            "Namespace",
            "Containers",
            "Labels",
            "Annotations",
            "Controlled By",
            "Managed Fields",
            "Status",
            "Node",
            "Host IPs",
            "Pod IPs",
            "Service Account",
            "QoS Class",
            "Termination Grace Period",
            "Tolerations",
            "Conditions",
        ]
    );

    assert_eq!(
        field(&fields, "Created").unwrap().value.text(),
        "1m (1970-01-01T00:00:00Z)"
    );
    assert_eq!(
        field(&fields, "Name").unwrap().value.text(),
        "api-7d9f-ftg5t"
    );
    assert_eq!(field(&fields, "Namespace").unwrap().value.text(), "staging");
    assert_eq!(
        field(&fields, "Labels").unwrap().value,
        PodFieldValue::Chips(vec!["app=api".into(), "tier=backend".into()]),
        "one chip per label"
    );
    assert_eq!(
        field(&fields, "Annotations").unwrap().value,
        PodFieldValue::Chips(vec!["checked=yes".into(), "team=platform".into()])
    );
    assert_eq!(
        field(&fields, "Controlled By").unwrap().value,
        PodFieldValue::Link("ReplicaSet/api-7d9f".into())
    );
    assert_eq!(field(&fields, "Status").unwrap().value.text(), "Running");
    let PodFieldValue::ManagedFields(managed) = &field(&fields, "Managed Fields").unwrap().value
    else {
        panic!("managed fields render as PodFieldValue::ManagedFields");
    };
    assert_eq!(
        managed
            .iter()
            .map(|entry| format!("{} ({})", entry.manager, entry.operation))
            .collect::<Vec<_>>(),
        vec![
            "kubelet (Update)".to_string(),
            "kube-controller-manager (Update)".to_string(),
        ],
        "one entry per manager"
    );
    assert_eq!(field(&fields, "Node").unwrap().value.text(), "node-a");
    assert_eq!(field(&fields, "Host IPs").unwrap().value.text(), "10.0.0.1");
    assert_eq!(field(&fields, "Pod IPs").unwrap().value.text(), "10.1.0.7");
    assert_eq!(
        field(&fields, "Service Account").unwrap().value.text(),
        "api"
    );
    assert_eq!(
        field(&fields, "QoS Class").unwrap().value.text(),
        "Burstable"
    );
    assert_eq!(
        field(&fields, "Termination Grace Period")
            .unwrap()
            .value
            .text(),
        "30s"
    );
    assert_eq!(
        field(&fields, "Tolerations").unwrap().value,
        PodFieldValue::Collapsed(vec!["dedicated=api: NoSchedule".into()])
    );
}

/// Section 1.1: every field's section matches design.md's grouping table,
/// and every field the projection produces lands in exactly one tab.
#[test]
fn every_field_is_grouped_into_its_designed_section() {
    let fields = pod_fields(&rich_pod(), Timestamp::from_second(90).unwrap());

    let expected: &[(&str, DetailSection)] = &[
        ("Created", DetailSection::Overview),
        ("Name", DetailSection::Overview),
        ("Namespace", DetailSection::Overview),
        ("Containers", DetailSection::Containers),
        ("Labels", DetailSection::Overview),
        ("Annotations", DetailSection::Overview),
        ("Controlled By", DetailSection::Overview),
        ("Managed Fields", DetailSection::ManagedFields),
        ("Status", DetailSection::Overview),
        ("Node", DetailSection::Overview),
        ("Host IPs", DetailSection::Overview),
        ("Pod IPs", DetailSection::Overview),
        ("Service Account", DetailSection::Overview),
        ("QoS Class", DetailSection::Overview),
        ("Termination Grace Period", DetailSection::Overview),
        ("Tolerations", DetailSection::Overview),
        ("Conditions", DetailSection::Overview),
    ];
    for (label, section) in expected {
        assert_eq!(
            field(&fields, label).unwrap().section,
            *section,
            "{label} is in the wrong tab"
        );
    }
    assert_eq!(
        fields.len(),
        expected.len(),
        "every projected field is accounted for above"
    );
}

/// Section 3.1: a condition's badge tone follows its own status, so the
/// renderer cannot color a `False` condition green.
#[test]
fn condition_badges_tone_by_their_status() {
    let fields = pod_fields(&rich_pod(), Timestamp::from_second(0).unwrap());
    let PodFieldValue::Badges(badges) = &field(&fields, "Conditions").unwrap().value else {
        panic!("conditions render as badges");
    };

    assert_eq!(badges.len(), 2, "one badge per condition");
    assert_eq!(badges[0].condition, "Ready");
    assert_eq!(badges[0].status, "True");
    assert_eq!(badges[0].tone, BadgeTone::Good);
    assert_eq!(badges[1].condition, "PodScheduled");
    assert_eq!(badges[1].status, "False");
    assert_eq!(
        badges[1].tone,
        BadgeTone::Warning,
        "a condition that does not hold is not green"
    );
}

/// Section: containers and volumes are the section a pod detail view
/// exists to show - image, ready state, restart count, ports, and
/// resource requests/limits, joined from `spec.containers` and its
/// matching `status.container_statuses` entry by name.
#[test]
fn containers_join_spec_and_status_by_name() {
    use k8s_openapi::api::core::v1::{
        ContainerPort, ContainerState, ContainerStateRunning, ContainerStatus, ResourceRequirements,
    };
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;

    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().containers = vec![Container {
        name: "app".into(),
        image: Some("registry.example/app:1.2.3".into()),
        ports: Some(vec![ContainerPort {
            container_port: 8080,
            protocol: Some("TCP".into()),
            ..Default::default()
        }]),
        resources: Some(ResourceRequirements {
            requests: Some(
                [("cpu".to_string(), Quantity("100m".into()))]
                    .into_iter()
                    .collect(),
            ),
            limits: Some(
                [("memory".to_string(), Quantity("256Mi".into()))]
                    .into_iter()
                    .collect(),
            ),
            ..Default::default()
        }),
        ..Default::default()
    }];
    pod.status.as_mut().unwrap().container_statuses = Some(vec![ContainerStatus {
        name: "app".into(),
        ready: true,
        restart_count: 3,
        state: Some(ContainerState {
            running: Some(ContainerStateRunning::default()),
            ..Default::default()
        }),
        ..Default::default()
    }]);

    let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
    let PodFieldValue::Containers(containers) = &field(&fields, "Containers").unwrap().value else {
        panic!("containers render as PodFieldValue::Containers");
    };

    assert_eq!(containers.len(), 1);
    let app = &containers[0];
    assert_eq!(app.name, "app");
    assert_eq!(app.image, "registry.example/app:1.2.3");
    assert_eq!(app.ready, Some(true));
    assert_eq!(app.restart_count, 3);
    assert_eq!(app.state, "Running");
    assert_eq!(app.ports, vec!["8080/TCP"]);
    assert_eq!(app.requests, vec!["cpu=100m"]);
    assert_eq!(app.limits, vec!["memory=256Mi"]);
}

/// A container with no status yet (still scheduling) still gets a row -
/// it just does not know ready/restart/state.
#[test]
fn a_container_with_no_status_yet_still_gets_a_row() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().containers = vec![Container {
        name: "app".into(),
        ..Default::default()
    }];
    pod.status.as_mut().unwrap().container_statuses = None;

    let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
    let PodFieldValue::Containers(containers) = &field(&fields, "Containers").unwrap().value else {
        panic!("containers render as PodFieldValue::Containers");
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].ready, None);
    assert_eq!(containers[0].restart_count, 0);
    assert_eq!(containers[0].state, "Waiting");
}

/// Volumes name and type each source a pod actually uses.
#[test]
fn volumes_are_named_and_typed() {
    use k8s_openapi::api::core::v1::{
        ConfigMapVolumeSource, EmptyDirVolumeSource, PersistentVolumeClaimVolumeSource, Volume,
    };

    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().volumes = Some(vec![
        Volume {
            name: "config".into(),
            config_map: Some(ConfigMapVolumeSource {
                name: "app-config".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
        Volume {
            name: "data".into(),
            persistent_volume_claim: Some(PersistentVolumeClaimVolumeSource {
                claim_name: "app-data".into(),
                ..Default::default()
            }),
            ..Default::default()
        },
        Volume {
            name: "scratch".into(),
            empty_dir: Some(EmptyDirVolumeSource::default()),
            ..Default::default()
        },
    ]);

    let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
    let PodFieldValue::List(volumes) = &field(&fields, "Volumes").unwrap().value else {
        panic!("volumes render as PodFieldValue::List");
    };

    assert_eq!(
        volumes,
        &vec![
            "config: ConfigMap: app-config".to_string(),
            "data: PersistentVolumeClaim: app-data".to_string(),
            "scratch: EmptyDir".to_string(),
        ]
    );
}

/// A condition the cluster did not resolve reads as neither good nor bad.
#[test]
fn an_unresolved_condition_is_neither_good_nor_a_warning() {
    let mut pod = rich_pod();
    pod.status.as_mut().unwrap().conditions = Some(vec![PodCondition {
        type_: "Ready".into(),
        status: "Unknown".into(),
        ..Default::default()
    }]);

    let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
    let PodFieldValue::Badges(badges) = &field(&fields, "Conditions").unwrap().value else {
        panic!("conditions render as badges");
    };
    assert_eq!(badges[0].tone, BadgeTone::Unknown);
}

/// Section 3.2: rows whose source is absent are left out, not shown blank -
/// so a minimal pod reads as a short list, not a page of empty values.
#[test]
fn absent_fields_are_omitted_rather_than_blank() {
    let minimal = Pod {
        metadata: ObjectMeta {
            name: Some("bare".into()),
            namespace: Some("default".into()),
            ..Default::default()
        },
        ..Default::default()
    };

    let fields = pod_fields(&minimal, Timestamp::from_second(0).unwrap());
    for absent in [
        "Created",
        "Labels",
        "Annotations",
        "Controlled By",
        "Managed Fields",
        "Status",
        "Node",
        "Host IPs",
        "Pod IPs",
        "Service Account",
        "QoS Class",
        "Termination Grace Period",
        "Tolerations",
        "Conditions",
    ] {
        assert!(
            field(&fields, absent).is_none(),
            "{absent} has no source on a pod without one"
        );
    }
    assert_eq!(
        fields.iter().map(|f| f.label).collect::<Vec<_>>(),
        vec!["Name", "Namespace"],
        "only the fields the pod actually has"
    );
}

/// Section 3.2's "a single IP" case: a cluster reporting one address in the
/// singular field still gets a row, rather than no row at all.
#[test]
fn a_single_ip_reported_singularly_still_renders() {
    let mut pod = rich_pod();
    let status = pod.status.as_mut().unwrap();
    status.host_ips = None;
    status.host_ip = Some("192.168.1.5".into());
    status.pod_ips = None;
    status.pod_ip = Some("192.168.1.6".into());

    let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
    assert_eq!(
        field(&fields, "Host IPs").unwrap().value.text(),
        "192.168.1.5"
    );
    assert_eq!(
        field(&fields, "Pod IPs").unwrap().value.text(),
        "192.168.1.6"
    );
}

/// An `Exists` toleration matches on its key alone, so a value it does not
/// have must not be invented for it.
#[test]
fn an_exists_toleration_reads_by_its_key_alone() {
    let mut pod = rich_pod();
    pod.spec.as_mut().unwrap().tolerations = Some(vec![Toleration {
        key: Some("node-role.kubernetes.io/control-plane".into()),
        operator: Some("Exists".into()),
        effect: Some("NoSchedule".into()),
        ..Default::default()
    }]);

    let fields = pod_fields(&pod, Timestamp::from_second(0).unwrap());
    assert_eq!(
        field(&fields, "Tolerations").unwrap().value,
        PodFieldValue::Collapsed(vec![
            "node-role.kubernetes.io/control-plane: NoSchedule".into()
        ])
    );
}
