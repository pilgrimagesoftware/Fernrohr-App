//! `remembered-list-sort` in a generic list panel: the sort it starts with
//! (its own, the kind's remembered one, or Name ascending), the click and
//! keyboard cycle - ascending, descending, back to the default - and which of
//! those the kind remembers.

use super::{Harness, deployments, focus_table, harness_full, object, press, row_names};
use crate::config::workspace::SortState;
use crate::k8s::resource::object_list::table::SavedColumn;
use crate::util::shell::SortDefaults;
use gpui_kit::component::table::TableDelegate as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Modifiers, TestAppContext};

const KEY: &str = "apps/Deployment";

fn objects() -> Vec<kube::api::DynamicObject> {
    vec![
        object("web", Some("team-c")),
        object("api", Some("team-b")),
        object("db", Some("team-a")),
    ]
}

fn panel(cx: &mut TestAppContext, setup: impl FnOnce(&mut super::ObjectListPanel)) -> Harness {
    harness_full(cx, deployments(), "kind-dev", objects(), None, &[], setup)
}

fn sort_of(h: &mut Harness) -> Option<(String, bool)> {
    h.vcx.update(|_, cx| {
        let table = h.panel.read(cx).table.clone().expect("the table is drawn");
        table
            .read(cx)
            .delegate()
            .sort_state()
            .map(|(column, descending)| (column.to_string(), descending))
    })
}

fn sorted(column: &str, descending: bool) -> Option<(String, bool)> {
    Some((column.to_string(), descending))
}

fn remembered(h: &mut Harness) -> Option<SortState> {
    h.vcx.update(|_, cx| SortDefaults::get(cx, KEY))
}

fn remember(cx: &mut TestAppContext, column: &str, ascending: bool) {
    cx.update(|cx| {
        SortDefaults::record(
            cx,
            KEY,
            SortState {
                column: column.into(),
                ascending,
            },
        )
    });
}

fn state(column: &str, ascending: bool) -> Option<SortState> {
    Some(SortState {
        column: column.into(),
        ascending,
    })
}

/// Clicks the sort arrow in column `col_ix`'s header, as the mouse does. The
/// arrow is the header cell's last child, at its right end; only the cell is
/// registered for tests, so the click lands just inside that end.
fn click_header(h: &mut Harness, col_ix: usize) {
    let at = h.vcx.update(|window, cx| {
        window.render_frame(cx);
        let cell = window.find(("col-header", col_ix)).bounds();
        gpui_kit::point(cell.right() - gpui_kit::px(ARROW_INSET), cell.center().y)
    });
    h.vcx.simulate_click(at, Modifiers::none());
    h.vcx.run_until_parked();
}

/// How far inside a header cell's right edge its sort arrow's centre is: the
/// cell's padding plus half the 16px arrow.
const ARROW_INSET: f32 = 16.;

/// The direction column `col_ix`'s header arrow shows.
fn header_arrow(h: &mut Harness, col_ix: usize) -> Option<gpui_kit::component::table::ColumnSort> {
    h.vcx.update(|_, cx| {
        let table = h.panel.read(cx).table.clone().expect("the table is drawn");
        table.read(cx).delegate().column(col_ix, cx).sort
    })
}

#[gpui_kit::test]
async fn a_first_list_of_a_kind_opens_on_name_ascending(cx: &mut TestAppContext) {
    let mut h = panel(cx, |_| {});
    assert_eq!(sort_of(&mut h), sorted("name", false));
    assert_eq!(row_names(&mut h), ["api", "db", "web"]);
    assert_eq!(remembered(&mut h), None, "falling back records nothing");
}

#[gpui_kit::test]
async fn a_new_list_opens_with_its_kinds_remembered_sort(cx: &mut TestAppContext) {
    remember(cx, "namespace", false);
    let mut h = panel(cx, |_| {});
    assert_eq!(sort_of(&mut h), sorted("namespace", true));
    assert_eq!(row_names(&mut h), ["web", "api", "db"]);
}

#[gpui_kit::test]
async fn a_remembered_column_the_kind_lacks_falls_back_to_name(cx: &mut TestAppContext) {
    remember(cx, "replicas-gone", false);
    let mut h = panel(cx, |_| {});
    assert_eq!(sort_of(&mut h), sorted("name", false));
}

#[gpui_kit::test]
async fn a_restored_list_keeps_its_own_sort_and_records_nothing(cx: &mut TestAppContext) {
    remember(cx, "namespace", false);
    let mut h = panel(cx, |panel| {
        panel.initial_sort = Some(("name".into(), true));
    });
    assert_eq!(sort_of(&mut h), sorted("name", true));
    assert_eq!(
        remembered(&mut h),
        state("namespace", false),
        "restoring left the remembered sort alone"
    );
}

#[gpui_kit::test]
async fn the_keyboard_cycles_a_column_and_the_kind_remembers_each_choice(cx: &mut TestAppContext) {
    let mut h = panel(cx, |_| {});
    focus_table(&mut h);

    press(&mut h.vcx, "shift-.");
    assert_eq!(
        sort_of(&mut h),
        sorted("namespace", false),
        "the next column, ascending"
    );
    assert_eq!(remembered(&mut h), state("namespace", true));

    press(&mut h.vcx, "o");
    assert_eq!(sort_of(&mut h), sorted("namespace", true));
    assert_eq!(row_names(&mut h), ["web", "api", "db"]);

    press(&mut h.vcx, "o");
    assert_eq!(
        sort_of(&mut h),
        sorted("name", false),
        "back to the default"
    );
    assert_eq!(
        remembered(&mut h),
        state("name", true),
        "returning to the default is a choice too"
    );

    press(&mut h.vcx, "shift-,");
    assert_eq!(
        sort_of(&mut h),
        sorted("age", false),
        "the previous column wraps to the last"
    );
}

#[gpui_kit::test]
async fn the_default_is_name_however_the_columns_are_ordered(cx: &mut TestAppContext) {
    // Age dragged ahead of Name.
    let mut h = panel(cx, |panel| {
        panel.initial_layout = vec![SavedColumn {
            id: "age".into(),
            width: 80.,
        }];
    });
    assert_eq!(sort_of(&mut h), sorted("name", false));
    focus_table(&mut h);

    press(&mut h.vcx, "shift-,");
    assert_eq!(
        sort_of(&mut h),
        sorted("age", false),
        "Age is displayed before Name"
    );
    press(&mut h.vcx, "o o");
    assert_eq!(
        sort_of(&mut h),
        sorted("name", false),
        "the default is the kind's first column, not the first displayed"
    );
}

#[gpui_kit::test]
async fn header_clicks_cycle_and_the_arrows_follow(cx: &mut TestAppContext) {
    let mut h = panel(cx, |_| {});
    // Name, Namespace, ...: Namespace is the second column.
    click_header(&mut h, 1);
    assert_eq!(
        sort_of(&mut h),
        sorted("namespace", false),
        "a first click is ascending"
    );
    assert_eq!(
        header_arrow(&mut h, 1),
        Some(gpui_kit::component::table::ColumnSort::Ascending)
    );

    click_header(&mut h, 1);
    assert_eq!(sort_of(&mut h), sorted("namespace", true));

    click_header(&mut h, 1);
    assert_eq!(
        sort_of(&mut h),
        sorted("name", false),
        "the third click is the default"
    );
    assert_eq!(
        header_arrow(&mut h, 0),
        Some(gpui_kit::component::table::ColumnSort::Ascending),
        "Name's arrow shows the default sort"
    );
    assert_eq!(
        header_arrow(&mut h, 1),
        Some(gpui_kit::component::table::ColumnSort::Default)
    );
    assert_eq!(remembered(&mut h), state("name", true));

    // Clicking the default column alternates.
    click_header(&mut h, 0);
    assert_eq!(sort_of(&mut h), sorted("name", true));
    click_header(&mut h, 0);
    assert_eq!(sort_of(&mut h), sorted("name", false));
}

#[gpui_kit::test]
async fn a_sort_chosen_in_one_context_opens_the_next_list_of_its_kind(cx: &mut TestAppContext) {
    let mut first = panel(cx, |_| {});
    focus_table(&mut first);
    press(&mut first.vcx, "shift-. o");
    assert_eq!(sort_of(&mut first), sorted("namespace", true));

    let mut second = harness_full(cx, deployments(), "kind-prod", objects(), None, &[], |_| {});
    assert_eq!(
        sort_of(&mut second),
        sorted("namespace", true),
        "a list of the same kind in another context starts with it"
    );
}
