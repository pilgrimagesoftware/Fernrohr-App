//! Clipped text in a list table's cells (#390): a cell whose text is wider than
//! its column ends with an ellipsis, and hovering it shows the full value.
//!
//! The ellipsis is a style ([`ellipsize`]); the tooltip is added only to a cell
//! that really is clipped ([`with_full_value`]), found by measuring the text in
//! the data font as `table_fit` does, so a short value carries no tooltip. The
//! keyboard route to a full value is each list's detail panel (`D`, or Enter where bound), which
//! shows every value in full.

use crate::ui::table_fit::{FitColumns, table_size};
use crate::ui::typography::DATA_FAMILY;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

/// `cell` as a single line that ends with an ellipsis where the column cuts it.
pub fn ellipsize(cell: Div) -> Div {
    cell.whitespace_nowrap().overflow_hidden().text_ellipsis()
}

/// Whether text `text_width` wide is cut off in a column `column_width` wide,
/// whose cells pad `padding` on both sides together and draw `adornment` (a
/// checkbox, a status dot) before the text.
pub fn is_clipped(
    text_width: Pixels,
    column_width: Pixels,
    padding: Pixels,
    adornment: Pixels,
) -> bool {
    text_width + adornment + padding > column_width
}

/// `cell` - which [`ellipsize`] styled and which shows `text` - with a tooltip
/// reading `text` in full when the column cuts it off, as the element for the
/// table's cell `(row_ix, col_ix)`.
pub fn with_full_value<D: FitColumns>(
    delegate: &D,
    row_ix: usize,
    col_ix: usize,
    text: &str,
    cell: Div,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if text.is_empty() {
        return cell.into_any_element();
    }
    let theme = cx.theme();
    let run = TextRun {
        len: text.len(),
        font: font(DATA_FAMILY),
        color: theme.foreground,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let text_width = window
        .text_system()
        .shape_line(text.to_string().into(), theme.font_size, &[run], None)
        .width;
    let padding = table_size(cx).table_cell_padding();
    let adornment = delegate.cell_adornment(row_ix, col_ix, window.rem_size(), cx);
    let column_width = delegate.column(col_ix, cx).width;
    if !is_clipped(
        text_width,
        column_width,
        padding.left + padding.right,
        adornment,
    ) {
        return cell.into_any_element();
    }
    crate::ui::icon_tooltip::with_tooltip_text(
        SharedString::from(format!("cell-full-value-{row_ix}-{col_ix}")),
        text.to_string(),
        cell.w_full(),
    )
    .w_full()
    .min_w_0()
    .overflow_hidden()
    .into_any_element()
}

#[cfg(test)]
mod tests;
