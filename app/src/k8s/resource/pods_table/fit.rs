//! Fitting the Pods table's columns (`ui::table_fit`): where the header cells
//! were drawn, a fitted width written back to its column, and what a cell
//! draws besides its text.

use super::{PodColumn, PodTableDelegate};
use crate::ui::table_fit::{checkbox_adornment, status_dot_adornment};
use gpui_kit::{App, Pixels};

impl crate::ui::table_fit::FitColumns for PodTableDelegate {
    fn header_bounds(&self) -> &crate::ui::table_fit::HeaderBounds {
        &self.header
    }

    fn set_column_width(&mut self, col_ix: usize, width: Pixels) {
        if let Some(column) = self.columns.get(col_ix).copied() {
            self.set_width(column, width);
        }
    }

    /// The first column's bulk checkbox, and Ready's dot - what `render_td`
    /// draws besides the text.
    fn cell_adornment(&self, _row_ix: usize, col_ix: usize, rem: Pixels, _cx: &App) -> Pixels {
        let checkbox = if col_ix == 0 {
            checkbox_adornment(rem)
        } else {
            Pixels::ZERO
        };
        let dot = match self.columns.get(col_ix) {
            Some(PodColumn::Ready) => status_dot_adornment(rem),
            _ => Pixels::ZERO,
        };
        checkbox + dot
    }
}
