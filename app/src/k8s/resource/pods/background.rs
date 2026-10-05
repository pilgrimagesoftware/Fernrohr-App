//! Open in Background for the Pods panel (`open-in-background` 2.1): the selected
//! row's pod detail opened as an inactive tab, the selection - and so
//! `SelectedPod` - left as it is. The mouse route is `ui::background_rows`.

use super::*;

impl PodsPanel {
    pub(super) fn on_action_open_in_background(
        &mut self,
        _: &OpenInBackground,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.pod_table.clone() else {
            return;
        };
        let table = table.read(cx);
        let Some(row) = table
            .selected_row()
            .and_then(|row_ix| table.delegate().rows().get(row_ix))
        else {
            return;
        };
        let open = row.selection.background_open();
        window.dispatch_action(Box::new(open), cx);
    }
}

#[cfg(test)]
mod tests;
