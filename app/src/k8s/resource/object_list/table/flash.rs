//! The list table's part in flashing changed rows (#275): its rows as
//! [`FlashRow`]s, and taking new rows through its [`RowFlashes`].

use super::{ListRow, ObjectTableDelegate};
use crate::ui::row_flash::{Changes, FlashRow, RowFlashes};
use gpui_kit::component::table::TableState;
use gpui_kit::*;

/// An object's row flashes by its uid, and draws what its name, namespace,
/// creation time, kind cells and conditions say - not its `resourceVersion`,
/// nor its age, which moves on by itself.
impl FlashRow for ListRow {
    fn flash_uid(&self) -> &str {
        &self.object.uid
    }

    fn shows_same(&self, other: &Self) -> bool {
        let (a, b) = (&self.object, &other.object);
        a.name == b.name
            && a.namespace == b.namespace
            && a.created == b.created
            && a.cells == b.cells
            && a.conditions == b.conditions
    }
}

impl ObjectTableDelegate {
    /// [`Self::set_rows`], flashing what the store's live `changes` did to the
    /// rows drawn until now, at `at` on the executor's clock.
    pub(in crate::k8s::resource::object_list) fn set_rows_flashing(
        &mut self,
        changes: Changes,
        rows: Vec<ListRow>,
        at: std::time::Instant,
    ) {
        let rows = self.flashes.update(changes, &self.natural, rows, at);
        self.set_rows(rows);
    }

    pub(in crate::k8s::resource::object_list) fn flashes(&self) -> &RowFlashes<ListRow> {
        &self.flashes
    }

    pub(in crate::k8s::resource::object_list) fn flashes_mut(
        &mut self,
    ) -> &mut RowFlashes<ListRow> {
        &mut self.flashes
    }

    /// Row `row_ix`, unless it is a deleted row still drawn for its flash -
    /// which no action, check or open reaches (#275).
    pub(in crate::k8s::resource) fn live_row(&self, row_ix: usize) -> Option<&ListRow> {
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
    pub(in crate::k8s::resource::object_list) fn flash_of(
        &self,
        uid: &str,
    ) -> Option<crate::ui::row_flash::RowChange> {
        self.flashes.change(uid)
    }
}
