//! The Pods table's part in flashing changed rows (#275): its rows as
//! [`FlashRow`]s, and taking new rows through its [`RowFlashes`].

use super::{PodTableDelegate, PodTableRow};
use crate::ui::row_flash::{Changes, FlashRow, RowFlashes};
use gpui_kit::component::table::TableState;
use gpui_kit::*;

/// A pod's row flashes by its uid, and draws its row's fields - its age aside,
/// which moves on by itself.
impl FlashRow for PodTableRow {
    fn flash_uid(&self) -> &str {
        &self.uid
    }

    fn shows_same(&self, other: &Self) -> bool {
        let ageless = |row: &PodTableRow| {
            let mut row = row.row.clone();
            row.age = String::new();
            row.age_secs = 0;
            row
        };
        ageless(self) == ageless(other)
    }
}

impl PodTableDelegate {
    /// [`Self::set_rows`], flashing what the store's live `changes` did to the
    /// rows drawn until now, at `at` on the executor's clock.
    pub(in crate::k8s::resource) fn set_rows_flashing(
        &mut self,
        changes: Changes,
        rows: Vec<PodTableRow>,
        at: std::time::Instant,
    ) {
        let rows = self.flashes.update(changes, &self.natural, rows, at);
        self.set_rows(rows);
    }

    pub(in crate::k8s::resource) fn flashes(&self) -> &RowFlashes<PodTableRow> {
        &self.flashes
    }

    pub(in crate::k8s::resource) fn flashes_mut(&mut self) -> &mut RowFlashes<PodTableRow> {
        &mut self.flashes
    }

    /// Row `row_ix`, unless it is a deleted row still drawn for its flash -
    /// which no action, check or open reaches (#275).
    pub(in crate::k8s::resource) fn live_row(&self, row_ix: usize) -> Option<&PodTableRow> {
        self.rows
            .get(row_ix)
            .filter(|row| !self.flashes.is_ghost(row.flash_uid()))
    }

    /// Row `row_ix`'s background while it flashes.
    pub(super) fn flash_tint(&self, row_ix: usize, cx: &Context<TableState<Self>>) -> Option<Hsla> {
        let row = self.rows.get(row_ix)?;
        self.flashes.tint(row.flash_uid(), cx)
    }

    /// The change `uid`'s row is flashing for. Test-only.
    #[cfg(test)]
    pub(in crate::k8s::resource) fn flash_of(
        &self,
        uid: &str,
    ) -> Option<crate::ui::row_flash::RowChange> {
        self.flashes.change(uid)
    }
}
