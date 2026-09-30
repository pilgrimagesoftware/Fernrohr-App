// Named imports rather than `use super::*`: `gpui_kit::*` (imported by the
// parent) re-exports its own `test` attribute macro, which would shadow the
// built-in `#[test]` for these plain synchronous tests.
use crate::k8s::resource::pods::{PodRow, PodSelection};

use super::pod_table_rows_fixture;

/// The pure per-column comparator `PodTableDelegate::resort` and
/// `sort_rows` both build on: this pins the column-to-field mapping for
/// every column, Ip and Node included, directly - each is exercised
/// against a pair of rows where it alone determines the order.
#[test]
fn compare_orders_rows_by_every_column() {
    use crate::k8s::resource::pods_table::{PodColumn, compare};
    use std::cmp::Ordering;

    // Every field of `a` sorts before the matching field of `b`, so each
    // column's comparator can be checked against the same pair.
    let a = PodRow {
        name: "a-name".into(),
        namespace: "ns-a".into(),
        ready: "0/2".into(),
        status: "Failed".into(),
        restarts: 1,
        age: String::new(),
        pod_ip: "10.0.0.1".into(),
        node: "node-a".into(),
        age_secs: 100,
    };
    let b = PodRow {
        name: "b-name".into(),
        namespace: "ns-b".into(),
        ready: "1/2".into(),
        status: "Running".into(),
        restarts: 2,
        age: String::new(),
        pod_ip: "10.0.0.2".into(),
        node: "node-b".into(),
        age_secs: 200,
    };

    for col in PodColumn::DEFAULT_ORDER {
        assert_eq!(
            compare(&a, &b, col),
            Ordering::Less,
            "{col:?} should order `a` before `b`"
        );
        assert_eq!(
            compare(&b, &a, col),
            Ordering::Greater,
            "{col:?} should order `b` after `a`"
        );
    }
}

/// `PodTableDelegate::resort` mirrors the header-click cycle
/// (`Default -> Descending -> Ascending -> Default`, see
/// `TableState::perform_sort`): each direction reorders `rows` by that
/// column, and cycling back to `Default` restores the order rows were
/// last supplied in - not merely whatever order sorting happened to leave
/// them in - for every sortable column.
#[test]
fn resort_orders_rows_and_default_restores_the_supplied_order() {
    use crate::k8s::resource::pods_table::{PodColumn, PodTableDelegate, compare};
    use gpui_kit::component::table::ColumnSort;

    fn names(rows: &[crate::k8s::resource::pods_table::PodTableRow]) -> Vec<&str> {
        rows.iter().map(|r| r.row.name.as_str()).collect()
    }

    for (col_ix, col) in PodColumn::DEFAULT_ORDER.into_iter().enumerate() {
        let natural = pod_table_rows_fixture();
        let mut delegate = PodTableDelegate::default();
        delegate.set_rows(natural.clone());

        delegate.resort(col_ix, ColumnSort::Descending);
        let mut want_desc = natural.clone();
        want_desc.sort_by(|a, b| compare(&b.row, &a.row, col));
        assert_eq!(
            names(delegate.rows()),
            names(&want_desc),
            "{col:?} descending"
        );

        delegate.resort(col_ix, ColumnSort::Ascending);
        let mut want_asc = natural.clone();
        want_asc.sort_by(|a, b| compare(&a.row, &b.row, col));
        assert_eq!(
            names(delegate.rows()),
            names(&want_asc),
            "{col:?} ascending"
        );

        delegate.resort(col_ix, ColumnSort::Default);
        assert_eq!(
            names(delegate.rows()),
            names(&natural),
            "{col:?} default should restore the supplied order"
        );
    }
}

/// A rows-only update - what `PodsPanel::sync_table` does every render -
/// must not undo an active sort: `set_rows` is `sync_table`'s call, and
/// unlike a full `TableState::refresh()` it has to keep applying the
/// sort that was active before the new rows arrived.
#[test]
fn set_rows_keeps_an_active_sort_applied() {
    use crate::k8s::resource::pods_table::{PodTableDelegate, compare};
    use gpui_kit::component::table::ColumnSort;

    let mut delegate = PodTableDelegate::default();
    delegate.set_rows(pod_table_rows_fixture());
    delegate.resort(0, ColumnSort::Ascending); // Name, ascending.

    // A fresh row arrives in an arbitrary position, as a live watch
    // update would deliver it - not already in sorted order.
    let mut updated = pod_table_rows_fixture();
    updated.insert(
        1,
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
    delegate.set_rows(updated.clone());

    let mut want = updated;
    want.sort_by(|a, b| {
        compare(
            &a.row,
            &b.row,
            crate::k8s::resource::pods_table::PodColumn::Name,
        )
    });
    let got: Vec<&str> = delegate
        .rows()
        .iter()
        .map(|r| r.row.name.as_str())
        .collect();
    let want_names: Vec<&str> = want.iter().map(|r| r.row.name.as_str()).collect();
    assert_eq!(
        got, want_names,
        "the sort applied before the row update still holds"
    );
}

/// After two column moves, the lookup `render_td` renders cells from
/// (`cell_text_at`) reads each visual position's *own* column - not the
/// column that used to sit there before the moves.
#[test]
fn moving_columns_renders_each_visual_position_from_its_own_column() {
    use crate::k8s::resource::pods_table::PodTableDelegate;

    let mut delegate = PodTableDelegate::default();
    delegate.set_rows(vec![crate::k8s::resource::pods_table::PodTableRow {
        row: PodRow {
            name: "web-1".into(),
            namespace: "default".into(),
            ready: "1/1".into(),
            status: "Running".into(),
            restarts: 0,
            age: "1m".into(),
            pod_ip: "10.0.0.9".into(),
            node: "node-z".into(),
            age_secs: 60,
        },
        selection: PodSelection {
            namespace: "default".into(),
            name: "web-1".into(),
            containers: Vec::new(),
            context_name: "ctx".into(),
        },
    }]);

    // Default order: [Name, Namespace, Ready, Status, Restarts, Age, Ip, Node].
    delegate.reorder_columns(7, 0); // Node to the front.
    delegate.reorder_columns(2, 0); // Namespace (now at index 2) to the front.
    // Now: [Namespace, Node, Name, Ready, Status, Restarts, Age, Ip].
    assert_eq!(
        delegate.cell_text_at(0, 0),
        "default",
        "col 0 is now Namespace"
    );
    assert_eq!(delegate.cell_text_at(0, 1), "node-z", "col 1 is now Node");
    assert_eq!(delegate.cell_text_at(0, 2), "web-1", "col 2 is now Name");
}
