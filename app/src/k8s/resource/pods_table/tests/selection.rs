// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use crate::k8s::resource::pods::{PodRow, PodSelection, SelectedPod};

use super::pod_table_rows_fixture;

/// `column(ix)` marks every column sortable, but only the active column's
/// direction survives - the other seven read back `ColumnSort::Default`
/// even while one column is actively sorted, which is what lets a future
/// `TableState::refresh()` redraw the same single indicator.
#[gpui_kit::test]
async fn column_reports_the_active_sort_on_the_active_column_only(
    cx: &mut gpui_kit::TestAppContext,
) {
    use crate::k8s::resource::pods_table::PodTableDelegate;
    use gpui_kit::component::table::{ColumnSort, TableDelegate as _};

    cx.update(|cx| {
        gpui_kit::init(cx);
        let mut delegate = PodTableDelegate::default();
        delegate.resort(2, ColumnSort::Descending);

        for col_ix in 0..delegate.columns_count(cx) {
            let column = delegate.column(col_ix, cx);
            if col_ix == 2 {
                assert_eq!(column.sort, Some(ColumnSort::Descending));
            } else {
                assert_eq!(
                    column.sort,
                    Some(ColumnSort::Default),
                    "column {col_ix} should not report the active column's sort"
                );
            }
        }
    });
}

/// The same sort-survives-a-row-update guarantee as
/// `set_rows_keeps_an_active_sort_applied`, but driven through a real
/// `TableState<PodTableDelegate>` in a window - the same object
/// `PodsPanel::sync_table` drives - rather than the delegate alone.
#[gpui_kit::test]
async fn a_row_update_keeps_a_real_table_states_rows_sorted(cx: &mut gpui_kit::TestAppContext) {
    use crate::k8s::resource::pods_table::PodTableDelegate;
    use gpui_kit::component::table::{ColumnSort, TableState};

    cx.update(gpui_kit::init);
    let window = cx.add_window(|window, cx| {
        TableState::new(PodTableDelegate::default(), window, cx)
            .sortable(true)
            .col_movable(true)
            .col_resizable(true)
    });

    window
        .update(cx, |table, _window, cx| {
            table.delegate_mut().set_rows(pod_table_rows_fixture());
            table.delegate_mut().resort(0, ColumnSort::Ascending);
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |table, _window, cx| {
            table.delegate_mut().set_rows(pod_table_rows_fixture());
            cx.notify();
        })
        .unwrap();

    let names: Vec<String> = window
        .update(cx, |table, _window, _cx| {
            table
                .delegate()
                .rows()
                .iter()
                .map(|r| r.row.name.clone())
                .collect()
        })
        .unwrap();
    assert_eq!(
        names,
        vec!["a-name", "b-name", "c-name", "d-name"],
        "the sort applied before the row update still holds"
    );
}

/// `PodTableDelegate::index_of` matches by namespace and name, not
/// position - the lookup `reselect` relies on to survive a sort or a row
/// update reordering `rows` underneath it.
#[test]
fn index_of_finds_the_row_by_namespace_and_name() {
    use crate::k8s::resource::pods_table::PodTableDelegate;

    let mut delegate = PodTableDelegate::default();
    delegate.set_rows(pod_table_rows_fixture());

    assert_eq!(
        delegate.index_of(&PodSelection {
            namespace: "ns-a".into(),
            name: "d-name".into(),
            containers: Vec::new(),
            context_name: "ctx".into(),
        }),
        Some(1),
    );
    assert_eq!(
        delegate.index_of(&PodSelection {
            namespace: "ns-a".into(),
            name: "missing".into(),
            containers: Vec::new(),
            context_name: "ctx".into(),
        }),
        None,
        "a name absent from that namespace's row should not match"
    );
}

/// `TableState` tracks its selection as a bare row index (see
/// `gpui-component`'s `TableState::selected_row`); a sort that moves the
/// selected pod to a different index must move the highlight with it,
/// via `PodTableDelegate::perform_sort`'s deferred `reselect` - the same
/// path a real header click drives.
#[gpui_kit::test]
async fn perform_sort_reselects_the_pod_that_moved(cx: &mut gpui_kit::TestAppContext) {
    use crate::k8s::resource::pods_table::PodTableDelegate;
    use gpui_kit::component::table::{ColumnSort, TableDelegate as _, TableState};

    cx.update(gpui_kit::init);
    let window = cx.add_window(|window, cx| {
        TableState::new(PodTableDelegate::default(), window, cx)
            .row_selectable(true)
            .sortable(true)
    });

    // "b-name" sits at natural index 0 (see `pod_table_rows_fixture`);
    // select it there, matching what a prior row click would have done.
    window
        .update(cx, |table, _window, cx| {
            table.delegate_mut().set_rows(pod_table_rows_fixture());
            table.set_selected_row(0, cx);
            cx.notify();
        })
        .unwrap();
    window
        .update(cx, |table, _window, _cx| {
            table.delegate_mut().remember_selection(Some(PodSelection {
                namespace: "ns-c".into(),
                name: "b-name".into(),
                containers: Vec::new(),
                context_name: "ctx".into(),
            }));
        })
        .unwrap();

    window
        .update(cx, |table, window, cx| {
            // Ascending by Name (column 0): "a-name" now sorts first,
            // pushing "b-name" from index 0 to index 1.
            table
                .delegate_mut()
                .perform_sort(0, ColumnSort::Ascending, window, cx);
        })
        .unwrap();
    cx.run_until_parked();

    let selected_name = window
        .update(cx, |table, _window, _cx| {
            table
                .selected_row()
                .map(|ix| table.delegate().rows()[ix].row.name.clone())
        })
        .unwrap();
    assert_eq!(
        selected_name.as_deref(),
        Some("b-name"),
        "selection should follow the pod that moved, not stay pinned to its old index"
    );
}

/// The same follow-the-pod guarantee as `perform_sort_reselects_the_pod_that_moved`,
/// but for a watch-driven row replacement (`PodsPanel::sync_table`'s call to
/// `set_rows`) rather than a sort - `reselect` is called directly there, not
/// deferred, since the caller already holds `&mut TableState`.
#[gpui_kit::test]
async fn set_rows_reselects_the_pod_that_moved(cx: &mut gpui_kit::TestAppContext) {
    use crate::k8s::resource::pods_table::{PodTableDelegate, reselect};
    use gpui_kit::component::table::TableState;

    cx.update(gpui_kit::init);
    let window = cx.add_window(|window, cx| {
        TableState::new(PodTableDelegate::default(), window, cx).row_selectable(true)
    });

    window
        .update(cx, |table, _window, cx| {
            table.delegate_mut().set_rows(pod_table_rows_fixture());
            table.set_selected_row(0, cx); // "b-name".
            cx.notify();
        })
        .unwrap();
    window
        .update(cx, |table, _window, _cx| {
            table.delegate_mut().remember_selection(Some(PodSelection {
                namespace: "ns-c".into(),
                name: "b-name".into(),
                containers: Vec::new(),
                context_name: "ctx".into(),
            }));
        })
        .unwrap();

    // Another Pods panel selecting a different pod publishes it app-wide;
    // this table must keep following its own pick, not the global.
    cx.update(|cx| {
        cx.set_global(SelectedPod(Some(PodSelection {
            namespace: "ns-a".into(),
            name: "d-name".into(),
            containers: Vec::new(),
            context_name: "ctx".into(),
        })));
    });

    // A fresh row arrives ahead of it, pushing "b-name" from index 0 to 1
    // - the same shape of change a live watch update delivers.
    let mut updated = pod_table_rows_fixture();
    updated.insert(
        0,
        crate::k8s::resource::pods_table::PodTableRow {
            row: PodRow {
                name: "aa-name".into(),
                namespace: "ns-e".into(),
                ready: "1/1".into(),
                status: "Running".into(),
                restarts: 0,
                age: String::new(),
                pod_ip: "10.0.0.5".into(),
                node: "node-e".into(),
                age_secs: 50,
            },
            selection: PodSelection {
                namespace: "ns-e".into(),
                name: "aa-name".into(),
                containers: Vec::new(),
                context_name: "ctx".into(),
            },
        },
    );

    window
        .update(cx, |table, _window, cx| {
            table.delegate_mut().set_rows(updated);
            reselect(table, cx);
            cx.notify();
        })
        .unwrap();

    let selected_name = window
        .update(cx, |table, _window, _cx| {
            table
                .selected_row()
                .map(|ix| table.delegate().rows()[ix].row.name.clone())
        })
        .unwrap();
    assert_eq!(
        selected_name.as_deref(),
        Some("b-name"),
        "selection should follow the pod to its new index, not stay pinned to the old one"
    );
}

/// When the selected pod is no longer among the new rows, `reselect`
/// clears the table's selection rather than leaving the highlight on
/// whichever pod now occupies the old index.
#[gpui_kit::test]
async fn set_rows_clears_selection_when_the_selected_pod_is_gone(
    cx: &mut gpui_kit::TestAppContext,
) {
    use crate::k8s::resource::pods_table::{PodTableDelegate, reselect};
    use gpui_kit::component::table::TableState;

    cx.update(gpui_kit::init);
    let window = cx.add_window(|window, cx| {
        TableState::new(PodTableDelegate::default(), window, cx).row_selectable(true)
    });

    window
        .update(cx, |table, _window, cx| {
            table.delegate_mut().set_rows(pod_table_rows_fixture());
            table.set_selected_row(0, cx); // "b-name".
            cx.notify();
        })
        .unwrap();
    window
        .update(cx, |table, _window, _cx| {
            table.delegate_mut().remember_selection(Some(PodSelection {
                namespace: "ns-c".into(),
                name: "b-name".into(),
                containers: Vec::new(),
                context_name: "ctx".into(),
            }));
        })
        .unwrap();

    // "b-name" is gone from the new rows entirely (e.g. the pod was
    // deleted between watch updates).
    let remaining: Vec<_> = pod_table_rows_fixture()
        .into_iter()
        .filter(|row| row.row.name != "b-name")
        .collect();

    window
        .update(cx, |table, _window, cx| {
            table.delegate_mut().set_rows(remaining);
            reselect(table, cx);
            cx.notify();
        })
        .unwrap();

    let selected_row = window
        .update(cx, |table, _window, _cx| table.selected_row())
        .unwrap();
    assert_eq!(
        selected_row, None,
        "a selected pod missing from the new rows should clear the selection, \
         not highlight whatever pod now sits at the old index"
    );
}
