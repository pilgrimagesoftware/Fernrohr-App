//! `visual-refresh-typography-spacing` 2.2: a pod table's cells are data
//! text and its headers frame text.

use super::pod_table_rows_fixture;
use crate::k8s::resource::pods_table::PodTableDelegate;
use crate::ui::typography::recorder::with_recorded_text;
use crate::ui::typography::{DATA_FAMILY, FRAME_FAMILY};
use gpui_kit::AppContext as _;
use gpui_kit::component::Root;
use gpui_kit::component::table::{DataTable, TableState};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};

/// The table as the Pods panel draws it: a `DataTable` over the state.
struct TableView(Entity<TableState<PodTableDelegate>>);

impl Render for TableView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(DataTable::new(&self.0))
    }
}

#[test]
fn cells_are_data_text_and_headers_frame_text() {
    with_recorded_text(|cx, recorded| {
        cx.update(|cx| {
            crate::util::test_ui::init(cx);
            crate::ui::theme::init(crate::config::ui::Theme::Light, cx);
        });
        let window = cx.add_window(|window, cx| {
            let table = cx.new(|cx| {
                let mut state = TableState::new(PodTableDelegate::default(), window, cx);
                state.delegate_mut().set_rows(pod_table_rows_fixture());
                state
            });
            let view = cx.new(|_| TableView(table));
            Root::new(view, window, cx)
        });
        // Through the untyped handle: a typed `WindowHandle<Root>` update
        // leases `Root`, which drawing the frame then tries to update again.
        let window: gpui_kit::AnyWindowHandle = window.into();
        window
            .update(cx, |_, window, cx| window.render_frame(cx))
            .unwrap();

        assert_eq!(recorded.family_of("b-name").as_ref(), DATA_FAMILY);
        assert_eq!(recorded.family_of("10.0.0.3").as_ref(), DATA_FAMILY);
        assert_eq!(recorded.family_of("Namespace").as_ref(), FRAME_FAMILY);
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    });
}
