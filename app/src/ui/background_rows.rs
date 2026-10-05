//! Opening a list row in the background with the mouse (`open-in-background` 2.2):
//! a click with the platform modifier (`cmd` on macOS, `ctrl` elsewhere), or a
//! middle-click, opens the row's object as an inactive tab and leaves the list's
//! selection where it was.
//!
//! A middle-click is an aux click, which the table ignores, so it selects nothing.
//! A modified click is harder: every click listener on a row runs, and the table's
//! own selects the clicked row and reports `SelectRow` - then `DoubleClickedRow`, on
//! a second click. So [`row`] notes the click on the delegate as it opens the
//! object, and the panel's table handler asks [`undo_select`] and
//! [`swallow_double_click`] before acting: the first puts the old selection back
//! (before anything follows the clicked row), the second drops the double-click's
//! foreground open.

use gpui_kit::component::table::{TableDelegate, TableState};
use gpui_kit::*;

/// A modified click in flight, between the row's click and the table's events.
#[derive(Default)]
pub struct BackgroundClick {
    /// The selection to put back: `Some(None)` when nothing was selected.
    restore: Option<Option<usize>>,
    /// The click was a double-click, whose `DoubleClickedRow` must not open.
    double: bool,
}

/// A table whose rows open in the background.
pub trait BackgroundRows: TableDelegate {
    /// Opens `row_ix`'s object in the background.
    fn open_in_background(&mut self, row_ix: usize, window: &mut Window, cx: &mut App);
    fn background_click(&mut self) -> &mut BackgroundClick;
}

/// A row element that opens in the background on a modified or middle click - the
/// delegate's `render_tr`.
pub fn row<D: BackgroundRows>(row_ix: usize, cx: &mut Context<TableState<D>>) -> Stateful<Div> {
    div()
        .id(("row", row_ix))
        .on_click(cx.listener(move |table, event: &ClickEvent, window, cx| {
            if !event.modifiers().secondary() {
                return;
            }
            let previous = table.selected_row();
            let click = table.delegate_mut().background_click();
            click.restore = Some(previous);
            click.double = event.click_count() >= 2;
            table.delegate_mut().open_in_background(row_ix, window, cx);
        }))
        .on_aux_click(cx.listener(move |table, event: &ClickEvent, window, cx| {
            if event.is_middle_click() {
                table.delegate_mut().open_in_background(row_ix, window, cx);
            }
        }))
}

/// For the panel's `SelectRow` handler: if this selection is a modified click's,
/// puts the previous selection back and returns `true` - the handler then does
/// nothing else with it.
pub fn undo_select<D: BackgroundRows>(
    table: &mut TableState<D>,
    cx: &mut Context<TableState<D>>,
) -> bool {
    let Some(previous) = table.delegate_mut().background_click().restore.take() else {
        return false;
    };
    match previous {
        Some(row_ix) => table.set_selected_row(row_ix, cx),
        None => table.clear_selection(cx),
    }
    true
}

/// For the panel's `DoubleClickedRow` handler: `true` when the double-click was a
/// modified one, already opened in the background.
pub fn swallow_double_click<D: BackgroundRows>(table: &mut TableState<D>) -> bool {
    std::mem::take(&mut table.delegate_mut().background_click().double)
}
