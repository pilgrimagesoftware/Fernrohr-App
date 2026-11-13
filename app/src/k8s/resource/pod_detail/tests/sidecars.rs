//! #353: the detail panel lists a pod's sidecars under their own heading,
//! between its app containers and its run-once init containers.

use crate::k8s::resource::pod_detail::fields::pod_fields;
use crate::k8s::resource::pod_detail::model::PodFieldValue;
use crate::k8s::resource::pods::sidecar_fixture as pod;
use jiff::Timestamp;

/// The detail lists the sidecar under its own heading, and the init
/// containers without it.
#[test]
fn the_detail_lists_sidecars_apart_from_init_containers() {
    let fields = pod_fields(&pod(), Timestamp::now());
    let names = |label: &str| -> Vec<String> {
        let field = fields
            .iter()
            .find(|field| field.label == label)
            .unwrap_or_else(|| panic!("a {label} field"));
        let PodFieldValue::Containers(containers) = &field.value else {
            panic!("{label} lists containers");
        };
        containers.iter().map(|c| c.name.clone()).collect()
    };
    assert_eq!(names("Containers"), ["app"]);
    assert_eq!(names("Sidecar Containers"), ["proxy"]);
    assert_eq!(names("Init Containers"), ["migrate"]);
    let labels: Vec<&str> = fields.iter().map(|field| field.label).collect();
    let at = |label| labels.iter().position(|l| *l == label).unwrap();
    assert!(at("Containers") < at("Sidecar Containers"));
    assert!(at("Sidecar Containers") < at("Init Containers"));
}
