// Named imports rather than `use super::*`: the parent's `gpui_kit::*` would
// shadow the built-in `#[test]` for these plain synchronous tests.
use super::super::row::ObjectRow;
use super::{ListColumn, ListRow, ObjectTableDelegate, SavedColumn, apply_layout, cell_text};
use crate::k8s::cluster::discovery::DiscoveredKind;
use gpui_kit::component::table::ColumnSort;
use gpui_kit::px;
use jiff::{SignedDuration, Timestamp};
use kube::core::GroupVersionKind;

fn kind(kind: &str, namespaced: bool) -> DiscoveredKind {
    DiscoveredKind {
        gvk: GroupVersionKind::gvk("", "v1", kind),
        plural: format!("{}s", kind.to_lowercase()),
        namespaced,
    }
}

fn ids(columns: &[ListColumn]) -> Vec<&str> {
    columns.iter().map(|column| column.id.as_ref()).collect()
}

/// A row named `name` in `namespace`, created `age_secs` before `now`.
fn row(name: &str, namespace: Option<&str>, age_secs: i64, now: Timestamp) -> ListRow {
    ListRow::new(
        ObjectRow {
            uid: format!("uid-{name}"),
            name: name.into(),
            namespace: namespace.map(Into::into),
            created: Some(now - SignedDuration::from_secs(age_secs)),
            cells: Vec::new(),
        },
        now,
    )
}

fn names(delegate: &ObjectTableDelegate) -> Vec<&str> {
    delegate
        .rows()
        .iter()
        .map(|row| row.object.name.as_str())
        .collect()
}

#[test]
fn a_namespaced_kind_has_name_namespace_and_age() {
    assert_eq!(
        ids(&ListColumn::for_kind(&kind("Service", true))),
        ["name", "namespace", "age"]
    );
}

#[test]
fn a_cluster_scoped_kind_has_no_namespace_column() {
    assert_eq!(
        ids(&ListColumn::for_kind(&kind("Node", false))),
        ["name", "age"]
    );
}

/// Age sorts by how old the object is, not by its text: "2h" sorts after "30s",
/// though it would come first as a string.
#[test]
fn age_sorts_by_seconds_not_by_text() {
    let now = Timestamp::now();
    let mut delegate = ObjectTableDelegate::new(ListColumn::for_kind(&kind("Service", true)));
    delegate.set_rows(vec![
        row("two-hours", Some("a"), 7200, now),
        row("thirty-seconds", Some("a"), 30, now),
        row("five-minutes", Some("a"), 300, now),
    ]);
    let age = delegate
        .columns()
        .iter()
        .position(|column| column.id == "age")
        .unwrap();

    delegate.resort(age, ColumnSort::Ascending);
    assert_eq!(
        names(&delegate),
        ["thirty-seconds", "five-minutes", "two-hours"]
    );
    assert_eq!(
        cell_text(&delegate.rows()[2], &delegate.columns()[age]),
        "2h"
    );

    delegate.resort(age, ColumnSort::Descending);
    assert_eq!(
        names(&delegate),
        ["two-hours", "five-minutes", "thirty-seconds"]
    );

    delegate.resort(age, ColumnSort::Default);
    assert_eq!(
        names(&delegate),
        ["two-hours", "thirty-seconds", "five-minutes"],
        "unsorted restores the incoming order"
    );
}

#[test]
fn name_sorts_alphabetically_and_survives_new_rows() {
    let now = Timestamp::now();
    let mut delegate = ObjectTableDelegate::new(ListColumn::for_kind(&kind("Node", false)));
    delegate.set_rows(vec![
        row("node-c", None, 1, now),
        row("node-a", None, 1, now),
    ]);
    delegate.resort(0, ColumnSort::Ascending);
    assert_eq!(names(&delegate), ["node-a", "node-c"]);

    // A watch update replaces the rows; the sort still applies.
    delegate.set_rows(vec![
        row("node-c", None, 1, now),
        row("node-b", None, 1, now),
        row("node-a", None, 1, now),
    ]);
    assert_eq!(names(&delegate), ["node-a", "node-b", "node-c"]);
}

/// The selection follows the object, not its position, across a re-sort.
#[test]
fn the_selection_follows_its_object_across_a_sort() {
    let now = Timestamp::now();
    let mut delegate = ObjectTableDelegate::new(ListColumn::for_kind(&kind("Service", true)));
    delegate.set_rows(vec![
        row("b", Some("x"), 1, now),
        row("a", Some("x"), 1, now),
    ]);
    delegate.remember_selection(0);
    assert_eq!(delegate.selected_index(), Some(0));

    delegate.resort(0, ColumnSort::Ascending);
    assert_eq!(
        delegate.selected_index(),
        Some(1),
        "`b` moved to the second row"
    );

    delegate.set_rows(vec![row("a", Some("x"), 1, now)]);
    assert_eq!(delegate.selected_index(), None, "`b` is gone");
}

#[test]
fn a_saved_layout_restores_order_and_widths_and_tolerates_change() {
    let columns = ListColumn::for_kind(&kind("Service", true));
    let saved = vec![
        SavedColumn {
            id: "age".into(),
            width: 90.,
        },
        SavedColumn {
            id: "gone".into(),
            width: 50.,
        },
        SavedColumn {
            id: "name".into(),
            width: 300.,
        },
    ];
    let laid_out = apply_layout(columns, &saved);
    assert_eq!(
        ids(&laid_out),
        ["age", "name", "namespace"],
        "unsaved columns follow"
    );
    assert_eq!(laid_out[0].width, px(90.));
    assert_eq!(laid_out[1].width, px(300.));

    let mut delegate = ObjectTableDelegate::new(laid_out);
    delegate.reorder_columns(2, 0);
    delegate.set_widths(&[px(120.), px(95.), px(310.)]);
    let layout = delegate.layout();
    let saved_ids: Vec<&str> = layout.iter().map(|column| column.id.as_str()).collect();
    assert_eq!(saved_ids, ["namespace", "age", "name"]);
    assert_eq!(layout[2].width, 310.);
}
