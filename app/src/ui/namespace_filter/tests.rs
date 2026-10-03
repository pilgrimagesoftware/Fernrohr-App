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
