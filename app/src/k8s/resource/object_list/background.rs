//! Open in Background for a list panel (`open-in-background` 2.1-2.2): the
//! selected row's object - from the keyboard - or a clicked row's - from the mouse,
//! through `ui::background_rows` - opened as an inactive tab, the selection left
//! where it is.

use super::commands::{OpenInBackground, OpenListedObject};
use super::panel::ObjectListPanel;
use super::table::ListRow;
use crate::ui::nav::{ObjectTarget, OpenMode};
use gpui_kit::*;

impl ObjectListPanel {
    /// What opening `row` in the background dispatches. Built from the panel's
    /// kind and context once, for the table's rows to call while it is mid-update.
    pub(super) fn background_opener(&self) -> impl Fn(&ListRow) -> OpenListedObject + 'static {
        let kind = self.kind.clone();
        let context_name = self.scope.context_name.clone();
        move |row| OpenListedObject {
            context_name: context_name.clone(),
            target: ObjectTarget {
                kind: kind.clone(),
                namespace: row.object.namespace.clone(),
                name: row.object.name.clone(),
            },
            view: None,
            mode: OpenMode::Background,
        }
    }

    pub(super) fn on_action_open_in_background(
        &mut self,
        _: &OpenInBackground,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(table), Some(row_ix)) = (self.table.clone(), self.selected_row(cx)) else {
            return;
        };
        let Some(row) = table.read(cx).delegate().rows().get(row_ix).cloned() else {
            return;
        };
        window.dispatch_action(Box::new(self.background_opener()(&row)), cx);
    }
}
