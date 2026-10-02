//! `0-column-autofit`: the Pods table's Name column defaults wide enough for a
//! Deployment pod's name, and a double-click on a header divider - drawn in a
//! real window, clicked with a real double-click - fits the column on its left.

use super::pod_table_rows_fixture;
use crate::k8s::resource::pods_table::{PodColumn, PodTableDelegate, PodTableRow, data_table};
use gpui_kit::component::Size;
use gpui_kit::component::table::{TableDelegate as _, TableState};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement as _, Pixels, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, point, px,
};

struct TableView(Entity<TableState<PodTableDelegate>>);

impl Render for TableView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(data_table(&self.0, cx))
    }
}

/// A window drawing a Pods table over `rows`, laid out.
fn table_window(
    cx: &mut TestAppContext,
    rows: Vec<PodTableRow>,
) -> (VisualTestContext, Entity<TableState<PodTableDelegate>>) {
    cx.update(gpui_kit::init);
    let mut built = None;
    let window: AnyWindowHandle = cx
        .add_window(|window, cx| {
            let table = cx.new(|cx| {
                let mut state = TableState::new(PodTableDelegate::default(), window, cx)
                    .col_resizable(true)
                    .sortable(true);
                state.delegate_mut().set_rows(rows);
                state
            });
            built = Some(table.clone());
            TableView(table)
        })
        .into();
    // Twice: the body is a virtual list that lays its rows out on its second frame.
    for _ in 0..2 {
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    (
        VisualTestContext::from_window(window, cx),
        built.expect("the window built its table"),
    )
}

fn name_width(table: &Entity<TableState<PodTableDelegate>>, vcx: &mut VisualTestContext) -> Pixels {
    vcx.update(|_, cx| table.read(cx).delegate().column(0, cx).width)
}

/// Double-clicks the divider on the right of column `col_ix`, where the table
/// draws it: the header cell starts the cell padding before its content, and
/// is the column's width wide.
fn double_click_divider(
    table: &Entity<TableState<PodTableDelegate>>,
    col_ix: usize,
    vcx: &mut VisualTestContext,
) {
    let selector: &'static str = format!("table-header-{col_ix}").leak();
    let header = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("column {col_ix}'s header is drawn"));
    let row_height = vcx.update(|_, cx| crate::ui::space::spacing(cx).row_height);
    let padding = Size::Size(row_height).table_cell_padding();
    let width = vcx.update(|_, cx| table.read(cx).delegate().column(col_ix, cx).width);
    let at = point(header.left() - padding.left + width, header.center().y);
    for click_count in [1, 2] {
        vcx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count,
            first_mouse: false,
        });
        vcx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count,
        });
    }
    vcx.run_until_parked();
}

/// (a): a typical Deployment pod name fits the Name column's default width.
#[test]
fn the_name_column_defaults_wide_enough_for_a_deployment_pod() {
    assert!(
        PodColumn::Name.default_width() >= 300.,
        "Name is {}px",
        PodColumn::Name.default_width()
    );
}

/// (b): the fixture's names are short, so fitting Name narrows it below its
/// default, though never below the header's own label.
#[gpui_kit::test]
fn double_clicking_the_name_divider_fits_short_names(cx: &mut TestAppContext) {
    let (mut vcx, table) = table_window(cx, pod_table_rows_fixture());
    let before = name_width(&table, &mut vcx);

    double_click_divider(&table, 0, &mut vcx);

    let after = name_width(&table, &mut vcx);
    assert!(
        after < before,
        "Name narrowed to fit: {before:?} -> {after:?}"
    );
    assert!(after >= px(40.), "but not below the minimum: {after:?}");
}

/// (b): a name longer than the default widens the column past it.
#[gpui_kit::test]
fn double_clicking_the_name_divider_fits_a_long_name(cx: &mut TestAppContext) {
    let mut rows = pod_table_rows_fixture();
    rows[0].row.name =
        "payments-reconciliation-worker-canary-6b8f9c7d4e-q7w2z-with-a-long-suffix".into();
    let (mut vcx, table) = table_window(cx, rows);
    let before = name_width(&table, &mut vcx);

    double_click_divider(&table, 0, &mut vcx);

    let after = name_width(&table, &mut vcx);
    assert!(
        after > before,
        "Name widened to fit: {before:?} -> {after:?}"
    );
}

/// A single click on the divider fits nothing - only the double-click does.
#[gpui_kit::test]
fn a_single_click_on_the_divider_fits_nothing(cx: &mut TestAppContext) {
    let (mut vcx, table) = table_window(cx, pod_table_rows_fixture());
    let before = name_width(&table, &mut vcx);
    let header = vcx.debug_bounds("table-header-0").expect("drawn");
    let padding = Size::Size(vcx.update(|_, cx| crate::ui::space::spacing(cx).row_height))
        .table_cell_padding();
    let at = point(header.left() - padding.left + before, header.center().y);
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();

    assert_eq!(name_width(&table, &mut vcx), before);
}
