//! `PodTableDelegate::sort_state`/`set_sort_state` (`saved-panel-layouts` 1.6):
//! the shape a Pods panel saves and restores its active sort in.

use super::pod_table_rows_fixture;
use crate::k8s::resource::pods_table::PodTableDelegate;

fn delegate_with_rows() -> PodTableDelegate {
    let mut delegate = PodTableDelegate::default();
    delegate.set_rows(pod_table_rows_fixture());
    delegate
}

/// No active sort saves as `None`.
#[test]
fn no_active_sort_is_none() {
    let delegate = delegate_with_rows();
    assert_eq!(delegate.sort_state(), None);
}

/// Setting a saved sort both reorders the rows and is what `sort_state`
/// reports back - a round trip through the saved shape.
#[test]
fn a_set_sort_round_trips_and_reorders_rows() {
    let mut delegate = delegate_with_rows();

    delegate.set_sort_state("name", true);

    assert_eq!(delegate.sort_state(), Some(("name", true)));
    let names: Vec<&str> = delegate
        .rows()
        .iter()
        .map(|row| row.row.name.as_str())
        .collect();
    assert_eq!(names, ["d-name", "c-name", "b-name", "a-name"]);
}

/// An ascending saved sort is the mirror image.
#[test]
fn an_ascending_saved_sort_reorders_ascending() {
    let mut delegate = delegate_with_rows();

    delegate.set_sort_state("name", false);

    assert_eq!(delegate.sort_state(), Some(("name", false)));
    let names: Vec<&str> = delegate
        .rows()
        .iter()
        .map(|row| row.row.name.as_str())
        .collect();
    assert_eq!(names, ["a-name", "b-name", "c-name", "d-name"]);
}

/// A column id this build doesn't recognize is ignored: no sort is applied,
/// and the rows stay in their natural order.
#[test]
fn an_unknown_saved_column_is_ignored() {
    let mut delegate = delegate_with_rows();
    let natural: Vec<String> = delegate
        .rows()
        .iter()
        .map(|row| row.row.name.clone())
        .collect();

    delegate.set_sort_state("not-a-real-column", true);

    assert_eq!(delegate.sort_state(), None);
    let names: Vec<String> = delegate
        .rows()
        .iter()
        .map(|row| row.row.name.clone())
        .collect();
    assert_eq!(names, natural, "an unrecognized column applies no sort");
}
