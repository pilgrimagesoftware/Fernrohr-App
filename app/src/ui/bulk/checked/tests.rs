//! The checked set's own rules, apart from any table: toggling, range checks
//! from the last toggled row, and pruning what a watch update dropped.

use super::{CheckedRows, RowKey};

fn rows(count: usize) -> Vec<RowKey> {
    (0..count)
        .map(|index| RowKey::new(Some("shop"), &format!("web-{index}")))
        .collect()
}

fn names(checked: &CheckedRows, rows: &[RowKey]) -> Vec<String> {
    checked
        .in_order(rows)
        .into_iter()
        .map(|key| key.name.clone())
        .collect()
}

#[test]
fn a_toggle_checks_and_a_second_unchecks() {
    let rows = rows(3);
    let mut checked = CheckedRows::default();
    checked.toggle(rows[1].clone());
    assert_eq!(names(&checked, &rows), ["web-1"]);
    checked.toggle(rows[1].clone());
    assert!(checked.is_empty());
}

#[test]
fn a_range_checks_from_the_last_toggled_row_to_the_target() {
    let rows = rows(10);
    let mut checked = CheckedRows::default();
    checked.toggle(rows[3].clone());
    checked.check_range(&rows, &rows[7]);
    assert_eq!(
        names(&checked, &rows),
        ["web-3", "web-4", "web-5", "web-6", "web-7"]
    );
}

#[test]
fn a_range_runs_upward_too_and_continues_from_its_own_end() {
    let rows = rows(10);
    let mut checked = CheckedRows::default();
    checked.toggle(rows[7].clone());
    checked.check_range(&rows, &rows[5]);
    assert_eq!(names(&checked, &rows), ["web-5", "web-6", "web-7"]);
    checked.check_range(&rows, &rows[2]);
    assert_eq!(
        names(&checked, &rows),
        ["web-2", "web-3", "web-4", "web-5", "web-6", "web-7"]
    );
}

#[test]
fn a_range_with_no_start_checks_the_target_alone() {
    let rows = rows(5);
    let mut checked = CheckedRows::default();
    checked.check_range(&rows, &rows[2]);
    assert_eq!(names(&checked, &rows), ["web-2"]);
}

#[test]
fn a_reorder_keeps_the_same_objects_checked() {
    let mut rows = rows(5);
    let mut checked = CheckedRows::default();
    checked.toggle(rows[1].clone());
    checked.toggle(rows[3].clone());
    rows.reverse();
    checked.retain_listed(&rows);
    assert_eq!(names(&checked, &rows), ["web-3", "web-1"]);
}

#[test]
fn an_object_no_longer_listed_drops_out() {
    let rows = rows(4);
    let mut checked = CheckedRows::default();
    checked.toggle(rows[0].clone());
    checked.toggle(rows[2].clone());
    checked.retain_listed(&rows[1..]);
    assert_eq!(checked.len(), 1);
    assert!(checked.contains(&rows[2]));
}

#[test]
fn check_all_checks_every_row_whatever_was_checked_before() {
    let rows = rows(5);
    let mut checked = CheckedRows::default();
    checked.toggle(rows[3].clone());
    checked.check_all(&rows);
    assert_eq!(checked.len(), 5);
}

#[test]
fn check_all_keeps_where_the_next_range_starts() {
    let rows = rows(5);
    let mut checked = CheckedRows::default();
    checked.toggle(rows[1].clone());
    checked.check_all(&rows);
    checked.uncheck(&rows);
    checked.check_range(&rows, &rows[3]);
    assert_eq!(names(&checked, &rows), ["web-1", "web-2", "web-3"]);
}
