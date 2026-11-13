//! Fitting a list table's columns (`ui::table_fit`): where the header cells
//! were drawn, a fitted width written back to its column, and what a cell
//! draws besides its text.

use super::super::columns::Cell;
use super::ObjectTableDelegate;
use crate::ui::table_fit::{checkbox_adornment, status_dot_adornment};
use gpui_kit::{App, Pixels};

impl crate::ui::table_fit::FitColumns for ObjectTableDelegate {
    fn header_bounds(&self) -> &crate::ui::table_fit::HeaderBounds {
        &self.header
    }

    fn set_column_width(&mut self, col_ix: usize, width: Pixels) {
        if let Some(column) = self.columns.get_mut(col_ix) {
            column.width = width;
        }
    }

    /// The first column's bulk checkbox, and the dot before a readiness or
    /// Ready condition cell - what `render_td` draws besides the text.
    fn cell_adornment(&self, row_ix: usize, col_ix: usize, rem: Pixels, _cx: &App) -> Pixels {
        let checkbox = if col_ix == 0 {
            checkbox_adornment(rem)
        } else {
            Pixels::ZERO
        };
        let dotted = self
            .rows
            .get(row_ix)
            .zip(self.columns.get(col_ix))
            .and_then(|(row, column)| row.cell(column))
            .is_some_and(|cell| {
                matches!(cell, Cell::Readiness(..) | Cell::Condition(_)) && cell.tone().is_some()
            });
        checkbox
            + if dotted {
                status_dot_adornment(rem)
            } else {
                Pixels::ZERO
            }
    }
}
