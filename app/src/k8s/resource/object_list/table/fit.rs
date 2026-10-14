//! Fitting a list table's columns (`ui::table_fit`): where the header cells
//! were drawn, and a fitted width written back to its column.

use super::ObjectTableDelegate;
use gpui_kit::Pixels;

impl crate::ui::table_fit::FitColumns for ObjectTableDelegate {
    fn header_bounds(&self) -> &crate::ui::table_fit::HeaderBounds {
        &self.header
    }

    fn set_column_width(&mut self, col_ix: usize, width: Pixels) {
        if let Some(column) = self.columns.get_mut(col_ix) {
            column.width = width;
        }
    }
}
