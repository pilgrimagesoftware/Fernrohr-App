// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use crate::config::workspace::{NamespaceScope, SortState};
use crate::k8s::resource::pods::{matches_namespaces, pod_row, view_rows};
use jiff::Timestamp;
use k8s_openapi::api::core::v1::Pod;

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

#[test]
fn single_namespace_scope_shows_only_its_pods() {
    let pods = mixed_namespace_fixture();
    let now = Timestamp::from_second(0).unwrap();

    let rows = view_rows(
        &pods,
        now,
        &NamespaceScope::Single("kube-system".into()),
        "",
        &default_sort(),
    );

    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.namespace == "kube-system"));
}

#[test]
fn all_namespace_scope_shows_every_pod() {
    let pods = mixed_namespace_fixture();
    let now = Timestamp::from_second(0).unwrap();

    let rows = view_rows(&pods, now, &NamespaceScope::All, "", &default_sort());

    assert_eq!(rows.len(), 3);
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

#[test]
fn name_filter_hides_non_matching_rows_and_clearing_restores_them() {
    let pods = vec![
        pod("u1", "nginx-1"),
        pod("u2", "nginx-2"),
        pod("u3", "web-1"),
    ];
    let now = Timestamp::from_second(0).unwrap();

    let filtered = view_rows(&pods, now, &NamespaceScope::All, "nginx", &default_sort());
    assert_eq!(filtered.len(), 2);
    assert!(filtered.iter().all(|r| r.name.contains("nginx")));

    let cleared = view_rows(&pods, now, &NamespaceScope::All, "", &default_sort());
    assert_eq!(cleared.len(), 3);
}

#[test]
fn age_sort_toggles_direction() {
    let pods = vec![
        pod_in("default", "u1", "oldest", 0),
        pod_in("default", "u2", "middle", 100),
        pod_in("default", "u3", "newest", 200),
    ];
    let now = Timestamp::from_second(1000).unwrap();

    let ascending = view_rows(
        &pods,
        now,
        &NamespaceScope::All,
        "",
        &SortState {
            column: "age".into(),
            ascending: true,
        },
    );
    let names: Vec<_> = ascending.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["newest", "middle", "oldest"]);

    let descending = view_rows(
        &pods,
        now,
        &NamespaceScope::All,
        "",
        &SortState {
            column: "age".into(),
            ascending: false,
        },
    );
    let names: Vec<_> = descending.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["oldest", "middle", "newest"]);
}
