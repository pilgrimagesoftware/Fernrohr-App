//! What the namespace list offers for a filter, and what a toggle leads to.

use super::{entries, toggled};

fn names() -> Vec<String> {
    ["default", "kube-public", "kube-system", "payments"]
        .map(String::from)
        .to_vec()
}

#[test]
fn a_filter_keeps_all_namespaces_and_matches_ignoring_case() {
    assert_eq!(
        entries(&names(), "KUBE"),
        [
            None,
            Some("kube-public".to_string()),
            Some("kube-system".to_string())
        ]
    );
}

#[test]
fn an_empty_filter_lists_everything_and_no_match_lists_only_all() {
    assert_eq!(entries(&names(), "").len(), 5);
    assert_eq!(entries(&names(), "zzz"), [None]);
}

#[test]
fn toggling_adds_removes_and_all_clears() {
    let picked = toggled(&[], Some("payments"));
    assert_eq!(picked, ["payments"]);
    let picked = toggled(&picked, Some("default"));
    assert_eq!(picked, ["default", "payments"], "kept sorted");
    assert_eq!(toggled(&picked, Some("default")), ["payments"]);
    assert!(toggled(&picked, None).is_empty());
}

/// #311: groups narrow by name, ignoring case, in their saved order.
#[test]
fn groups_match_their_name_ignoring_case() {
    use super::groups_matching;
    use crate::config::namespaces::NamespaceSetConfig;
    let groups = [
        NamespaceSetConfig {
            name: "Team Workloads".into(),
            namespaces: vec!["team-a".into()],
        },
        NamespaceSetConfig {
            name: "payments".into(),
            namespaces: vec!["payments".into()],
        },
    ];
    let names = |query: &str| -> Vec<String> {
        groups_matching(&groups, query)
            .map(|group| group.name.clone())
            .collect()
    };
    assert_eq!(names(""), ["Team Workloads", "payments"]);
    assert_eq!(names("  team "), ["Team Workloads"]);
    assert!(names("nothing").is_empty());
}

/// A group's badge counts its namespaces, and those the cluster still has
/// when that's fewer - but not before the cluster's list is known.
#[test]
fn a_groups_count_says_how_many_it_still_has() {
    use super::member_count;
    let strings = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
    let members = strings(&["gone", "payments", "team-a"]);
    let cluster = strings(&["payments", "team-a", "team-b"]);
    assert_eq!(member_count(&members, &cluster), "2 of 3 namespaces");
    assert_eq!(member_count(&members, &[]), "3 namespaces");
    assert_eq!(
        member_count(&strings(&["payments"]), &cluster),
        "1 namespace"
    );
}

/// A group is checked only while the selection is exactly its namespaces.
#[test]
fn a_group_is_checked_only_for_exactly_its_namespaces() {
    use super::is_group;
    use crate::config::namespaces::NamespaceSetConfig;
    let group = NamespaceSetConfig {
        name: "team".into(),
        namespaces: vec!["team-b".into(), "team-a".into()],
    };
    let strings = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
    assert!(is_group(&strings(&["team-a", "team-b"]), &group));
    assert!(!is_group(&strings(&["team-a"]), &group));
    assert!(!is_group(&strings(&["team-a", "team-b", "team-c"]), &group));
    assert!(!is_group(&[], &group), "all namespaces isn't the group");
}
