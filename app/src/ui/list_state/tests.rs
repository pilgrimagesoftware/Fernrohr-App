//! Named imports, not `use super::*`: see `util/shell.rs` on the
//! macro-expansion budget.

use super::{FILTERED_EMPTY, TableArea, empty_text, table_area};
use crate::k8s::resource::load_phase::LoadPhase;

fn ns(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn the_first_list_shows_loading_whatever_has_arrived() {
    let area = table_area(LoadPhase::FirstLoad { received: 12 }, 12, 12, "Pods", None);
    assert_eq!(area, TableArea::Loading { received: 12 });
}

#[test]
fn a_loaded_list_with_nothing_says_so_by_scope() {
    let team_a = ns(&["team-a"]);
    assert_eq!(
        table_area(LoadPhase::Loaded, 0, 0, "Pods", Some(&team_a)),
        TableArea::Empty("No Pods in team-a".into())
    );
    assert_eq!(
        empty_text("Nodes", None),
        "No Nodes",
        "a cluster-scoped kind"
    );
    assert_eq!(empty_text("Pods", Some(&[])), "No Pods in any namespace");
    assert_eq!(
        empty_text("Pods", Some(&ns(&["a", "b"]))),
        "No Pods in 2 namespaces"
    );
}

#[test]
fn a_filter_that_hides_everything_says_so() {
    assert_eq!(
        table_area(LoadPhase::Loaded, 3, 0, "Pods", Some(&[])),
        TableArea::Empty(FILTERED_EMPTY.into())
    );
}

#[test]
fn a_refresh_keeps_showing_the_rows() {
    assert_eq!(
        table_area(
            LoadPhase::Refreshing { received: 0 },
            3,
            3,
            "Pods",
            Some(&[])
        ),
        TableArea::Table
    );
}
