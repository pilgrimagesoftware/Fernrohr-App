//! `namespace-sets` 1.1-1.2: the set rules, and the file's first run and a bad
//! parse.

use super::{NamespaceSetConfig, NamespaceSetsConfig, SetError};

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn team() -> NamespaceSetsConfig {
    let mut sets = NamespaceSetsConfig::default();
    sets.create("team-workloads", names(&["team-b", "team-a"]))
        .unwrap();
    sets
}

#[test]
fn a_created_set_keeps_its_namespaces_sorted() {
    let sets = team();
    assert_eq!(
        sets.find("team-workloads"),
        Some(&NamespaceSetConfig {
            name: "team-workloads".into(),
            namespaces: names(&["team-a", "team-b"]),
        })
    );
}

#[test]
fn a_duplicate_or_empty_name_is_refused() {
    let mut sets = team();
    assert_eq!(
        sets.create("team-workloads", names(&["payments"])),
        Err(SetError::NameTaken)
    );
    assert_eq!(
        sets.create("  ", names(&["payments"])),
        Err(SetError::EmptyName)
    );
    assert_eq!(sets.sets.len(), 1);
}

#[test]
fn an_empty_set_is_refused_and_the_last_namespace_stays() {
    let mut sets = team();
    assert_eq!(
        sets.create("nothing", Vec::new()),
        Err(SetError::NoNamespaces)
    );
    sets.apply_to("team-workloads", "team-b", false).unwrap();
    assert_eq!(
        sets.apply_to("team-workloads", "team-a", false),
        Err(SetError::NoNamespaces)
    );
    assert_eq!(sets.find("team-workloads").unwrap().namespaces, ["team-a"]);
}

/// A set may hold a namespace the connected cluster doesn't have: nothing
/// here knows the cluster's list, so it's kept like any other.
#[test]
fn a_namespace_absent_from_the_cluster_is_kept() {
    let mut sets = team();
    sets.apply_to("team-workloads", "retired-team", true)
        .unwrap();
    assert_eq!(
        sets.find("team-workloads").unwrap().namespaces,
        ["retired-team", "team-a", "team-b"]
    );
}

#[test]
fn renaming_keeps_the_place_and_refuses_a_taken_name() {
    let mut sets = team();
    sets.create("payments", names(&["payments"])).unwrap();
    assert_eq!(
        sets.rename("payments", "team-workloads"),
        Err(SetError::NameTaken)
    );
    sets.rename("team-workloads", "team").unwrap();
    assert_eq!(sets.sets[0].name, "team", "first still");
    sets.rename("team", "team").unwrap();
}

#[test]
fn removing_a_set() {
    let mut sets = team();
    assert!(sets.remove_set("team-workloads"));
    assert!(!sets.remove_set("team-workloads"));
    assert!(sets.sets.is_empty());
}

#[test]
fn a_scope_matches_a_set_only_exactly() {
    let sets = team();
    assert_eq!(
        sets.matching(&names(&["team-b", "team-a"]))
            .map(|set| set.name.as_str()),
        Some("team-workloads"),
        "in any order"
    );
    assert_eq!(sets.matching(&names(&["team-a"])), None, "a subset");
    assert_eq!(
        sets.matching(&names(&["team-a", "team-b", "team-c"])),
        None,
        "a superset"
    );
    assert_eq!(sets.matching(&[]), None, "all namespaces");
}

#[test]
fn first_run_writes_an_empty_file_and_sets_round_trip() {
    let path = crate::util::test_paths::temp_path("namespace-sets-config");
    let loaded: NamespaceSetsConfig = crate::config::load(&path);
    assert!(loaded.sets.is_empty());
    assert!(path.exists(), "first run writes the file");

    let sets = team();
    crate::config::save(&path, &sets).unwrap();
    let reloaded: NamespaceSetsConfig = crate::config::load(&path);
    assert_eq!(reloaded, sets);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_file_that_fails_to_parse_is_left_alone_and_yields_no_sets() {
    let path = crate::util::test_paths::temp_path("namespace-sets-config");
    std::fs::write(&path, "sets = \"not a list\"").unwrap();
    let loaded: NamespaceSetsConfig = crate::config::load(&path);
    assert!(loaded.sets.is_empty());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "sets = \"not a list\""
    );
    let _ = std::fs::remove_file(path);
}
