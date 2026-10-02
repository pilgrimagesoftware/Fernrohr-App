//! `visual-refresh-typography-spacing` 3.2, tables: a pod table's rows are
//! the spacing scale's row height.

use super::pod_table_rows_fixture;
use crate::k8s::resource::pods_table::{PodTableDelegate, data_table};
use crate::ui::space::{TextScale, spacing};
use gpui_kit::component::table::TableState;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div,
};

struct TableView(Entity<TableState<PodTableDelegate>>);

impl Render for TableView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(data_table(&self.0, cx))
    }
}

/// At 150% text, so the height is the scaled token and not gpui-component's
/// fixed default.
#[gpui_kit::test]
fn rows_are_the_scales_row_height(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        TextScale::new(1.5).expect("a valid scale").set(cx);
    });
    let window: AnyWindowHandle = cx
        .add_window(|window, cx| {
            let table = cx.new(|cx| {
                let mut state = TableState::new(PodTableDelegate::default(), window, cx);
                state.delegate_mut().set_rows(pod_table_rows_fixture());
                state
            });
            TableView(table)
        })
        .into();
    let expected = cx.update(|cx| spacing(cx).row_height);
    // Twice: the body is a virtual list, which lays its rows out only once it
    // has measured itself on the first frame.
    for _ in 0..2 {
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    let mut vcx = VisualTestContext::from_window(window, cx);
    let top = |vcx: &mut VisualTestContext, row: usize| {
        let selector: &'static str = format!("pod-cell-{row}-0").leak();
        vcx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("row {row}'s first cell is drawn"))
            .top()
    };
    let (first, second) = (top(&mut vcx, 0), top(&mut vcx, 1));
    assert_eq!(second - first, expected, "one row is the row height tall");
}
