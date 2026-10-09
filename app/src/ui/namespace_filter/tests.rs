//! What the namespace list offers for a filter, and what a toggle leads to.

use super::{apply_range, entries, toggled};

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

#[test]
fn range_selection_adds_when_anchor_is_unselected() {
    let namespaces = vec![
        Some("team-a".to_string()),
        Some("team-b".to_string()),
        Some("team-c".to_string()),
        Some("team-d".to_string()),
    ];
    // team-a is unselected (not in the selected list)
    let selected = vec!["team-d".to_string()];

    // Click team-a (to add), shift-click team-c
    // Since team-a is unselected, add all in range
    let result = apply_range(&namespaces, &selected, 0, 2);
    assert_eq!(
        result,
        ["team-a", "team-b", "team-c", "team-d"],
        "range from unselected anchor adds all in range"
    );
}

#[test]
fn range_selection_removes_when_anchor_is_selected() {
    let namespaces = vec![
        Some("team-a".to_string()),
        Some("team-b".to_string()),
        Some("team-c".to_string()),
        Some("team-d".to_string()),
    ];
    let selected = vec![
        "team-a".to_string(),
        "team-b".to_string(),
        "team-c".to_string(),
    ];

    // Click team-a (selected), shift-click team-c
    // Should remove all in range (team-a through team-c)
    let result = apply_range(&namespaces, &selected, 0, 2);
    assert!(
        result.is_empty(),
        "range from selected anchor removes all in range"
    );
}

#[test]
fn range_selection_follows_anchor_state_not_per_row_state() {
    let namespaces = vec![
        Some("team-a".to_string()),
        Some("team-b".to_string()),
        Some("team-c".to_string()),
        Some("team-d".to_string()),
    ];
    // Mixed state: team-a is selected, team-b and team-c are not
    let selected = vec!["team-a".to_string()];

    // Click team-a (selected), shift-click team-c
    // Should remove all in range, following anchor's action, not per-row states
    let result = apply_range(&namespaces, &selected, 0, 2);
    assert!(
        result.is_empty(),
        "mixed range follows anchor's action (remove)"
    );
}

#[test]
fn range_selection_with_no_anchor_defaults_to_first_row() {
    let namespaces = vec![
        Some("team-a".to_string()),
        Some("team-b".to_string()),
        Some("team-c".to_string()),
    ];
    let selected: Vec<String> = Vec::new();

    // First shift-click with no anchor: should range from row 0 to clicked row
    let result = apply_range(&namespaces, &selected, 0, 2);
    assert_eq!(
        result,
        ["team-a", "team-b", "team-c"],
        "range from first row when no anchor set"
    );
}
