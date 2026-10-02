//! Double-clicking a table header's column divider fits the column on its
//! left to its widest cell, header included - the Pods and list panels' tables.
//!
//! gpui-kit 0.7's `TableState` has no such gesture: its divider (the resize
//! handle band, `table/state.rs`) only drags, and it occludes the header under
//! it. So the double-click is caught here by a window-level mouse listener, and
//! the dividers are located from each header cell's drawn bounds, which the
//! delegate records through [`HeaderBounds::track`]. The fitted width goes back
//! through the delegate's own `column` width and `TableState::refresh`, the
//! only public way to set a column's width.
//!
//! The keyboard twin is [`FitAllColumns`], which fits every column at once and
//! is registered per panel (Pods, lists) in that panel's key context.
//!
//! This module owns the gesture, the action and the measuring; each delegate
//! owns its widths ([`FitColumns`]).

use crate::ui::typography::DATA_FAMILY;
use gpui_kit::component::Size;
use gpui_kit::component::table::{TableDelegate, TableState};
use gpui_kit::*;
use std::cell::RefCell;
use std::rc::Rc;

actions!(table_fit, [FitAllColumns]);

/// [`FitAllColumns`]' default key in the Pods and list panels.
pub const FIT_COLUMNS_KEY: &str = "=";

/// The View menu's one table-columns item: [`FitAllColumns`], answered by
/// whichever table panel has focus and greyed out otherwise. The Pods and
/// list panels register their own, panel-scoped copies for `=` and the
/// palette; those stay out of the menu bar (`menu-organization`), so this one
/// item stands for both.
pub fn register_commands(registry: &mut crate::command::CommandRegistry) {
    registry.register(crate::command::Command {
        id: "table.fit_columns",
        title: "Fit Columns to Contents",
        default_binding: "",
        context: None,
        action: Box::new(FitAllColumns),
        menu: Some(crate::command::MenuSlot::View(
            crate::command::ViewGroup::TableColumns,
        )),
    });
}

/// How far either side of a column boundary a double-click still counts as on
/// the divider - the half-width of gpui-kit's resize handle band
/// (`HANDLE_PADDING`, private to `table/state.rs`).
const DIVIDER_HALF_WIDTH: Pixels = px(4.);

/// Room left after a header's label for the sort icon beside it: the 12px
/// icon plus its 2px padding either side (`TableState::render_sort_icon`).
const SORT_ICON_WIDTH: Pixels = px(16.);

/// Slack added to a fitted width so the widest text isn't clipped by a
/// sub-pixel rounding of its own measure.
const FIT_SLACK: Pixels = px(4.);

/// The narrowest and widest a fitted column may become.
const FIT_MIN: Pixels = px(40.);
const FIT_MAX: Pixels = px(800.);

/// At most this many rows are measured, from the top of the displayed order:
/// enough for any list a person reads, without shaping every row of a huge one
/// on the click.
const FIT_ROWS: usize = 2_000;

/// Each header cell's drawn content bounds, by column index, as last painted.
/// Shared between the delegate (which records them) and the divider listener
/// (which reads them).
#[derive(Clone, Default)]
pub struct HeaderBounds(Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>);

impl HeaderBounds {
    /// `content`, as column `col_ix`'s header, recording its bounds each frame.
    pub fn track(&self, col_ix: usize, content: impl IntoElement) -> AnyElement {
        let cells = self.0.clone();
        div()
            .size_full()
            .relative()
            .debug_selector(move || format!("table-header-{col_ix}"))
            .child(content)
            .child(
                canvas(
                    move |bounds, _, _| {
                        let mut cells = cells.borrow_mut();
                        if cells.len() <= col_ix {
                            cells.resize(col_ix + 1, None);
                        }
                        cells[col_ix] = Some(bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    /// The column whose right-hand divider `position` is on, if any. Column
    /// `ix`'s cell starts `padding.left` before its header content and is
    /// `widths[ix]` wide, so its divider is at `content.left - padding.left +
    /// width`; the header row spans the content plus the vertical padding.
    fn divider_at(
        &self,
        position: Point<Pixels>,
        widths: &[Pixels],
        padding: Edges<Pixels>,
    ) -> Option<usize> {
        self.0
            .borrow()
            .iter()
            .zip(widths)
            .enumerate()
            .find_map(|(ix, (content, width))| {
                let content = (*content)?;
                let divider = content.left() - padding.left + *width;
                let in_row = position.y >= content.top() - padding.top
                    && position.y <= content.bottom() + padding.bottom;
                let on_divider = (position.x - divider).abs() <= DIVIDER_HALF_WIDTH;
                (in_row && on_divider).then_some(ix)
            })
    }
}

/// A table delegate whose columns can be fitted: it records its header
/// bounds and owns its column widths.
pub trait FitColumns: TableDelegate {
    fn header_bounds(&self) -> &HeaderBounds;

    /// Every column's current width, left to right.
    fn column_widths(&self, cx: &App) -> Vec<Pixels> {
        (0..self.columns_count(cx))
            .map(|ix| self.column(ix, cx).width)
            .collect()
    }

    /// Records `width` for column `col_ix`; [`fit_column`] then refreshes the
    /// table so it takes effect.
    fn set_column_width(&mut self, col_ix: usize, width: Pixels);
}

/// Fits column `col_ix` of `table` to its widest cell: the header label (in
/// the UI font, with room for the sort icon) or any measured row's text (in
/// the data font the cells draw in), plus the cell's padding.
pub fn fit_column<D: FitColumns>(
    table: &mut TableState<D>,
    col_ix: usize,
    size: Size,
    window: &mut Window,
    cx: &mut Context<TableState<D>>,
) {
    fit_columns(table, &[col_ix], size, window, cx);
}

/// [`FitAllColumns`]: fits every column of `table`, as [`fit_column`] fits one.
pub fn fit_all_columns<D: FitColumns>(
    table: &Entity<TableState<D>>,
    size: Size,
    window: &mut Window,
    cx: &mut App,
) {
    table.update(cx, |table, cx| {
        let columns: Vec<usize> = (0..table.delegate().columns_count(cx)).collect();
        fit_columns(table, &columns, size, window, cx);
    });
}

/// Fits each of `columns`, from one read of the table's text.
fn fit_columns<D: FitColumns>(
    table: &mut TableState<D>,
    columns: &[usize],
    size: Size,
    window: &mut Window,
    cx: &mut Context<TableState<D>>,
) {
    let (headers, rows) = table.dump_range(0..FIT_ROWS, cx);
    let style = window.text_style();
    let font_size = style.font_size.to_pixels(window.rem_size());
    let ui_font = style.font();
    let data_font = Font {
        family: DATA_FAMILY.into(),
        ..ui_font.clone()
    };
    let measure = |text: &str, font: &Font| -> Pixels {
        if text.is_empty() {
            return px(0.);
        }
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text.to_string().into(), font_size, &[run], None)
            .width
    };
    let padding = size.table_cell_padding();
    for &col_ix in columns {
        let Some(header) = headers.get(col_ix) else {
            continue;
        };
        let header_width = measure(header, &ui_font) + SORT_ICON_WIDTH;
        let widest = rows
            .iter()
            .filter_map(|row| row.get(col_ix))
            .map(|text| measure(text, &data_font))
            .fold(header_width, Pixels::max);
        let width = (widest + padding.left + padding.right + FIT_SLACK).clamp(FIT_MIN, FIT_MAX);
        table.delegate_mut().set_column_width(col_ix, width);
    }
    table.refresh(cx);
    cx.notify();
}

/// The size the Pods and list tables are drawn at: `ui::space`'s row height,
/// which also sets their cell padding.
pub fn table_size(cx: &App) -> Size {
    Size::Size(crate::ui::space::spacing(cx).row_height)
}

/// An empty element that, while drawn, listens window-wide for a double-click
/// on one of `table`'s header dividers and fits the column on its left.
/// Draw it beside the table; `size` is the table's own size, which sets its
/// cell padding.
pub fn divider_double_click<D: FitColumns>(
    table: &Entity<TableState<D>>,
    size: Size,
) -> impl IntoElement {
    let table = table.clone();
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if !phase.bubble() || event.button != MouseButton::Left || event.click_count != 2 {
                    return;
                }
                let state = table.read(cx);
                let widths = state.delegate().column_widths(cx);
                let Some(col_ix) = state.delegate().header_bounds().divider_at(
                    event.position,
                    &widths,
                    size.table_cell_padding(),
                ) else {
                    return;
                };
                cx.stop_propagation();
                table.update(cx, |table, cx| fit_column(table, col_ix, size, window, cx));
            });
        },
    )
    .absolute()
    .size_0()
}

#[cfg(test)]
mod tests;
