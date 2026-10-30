//! A Pods row's context menu (`pod-quick-look` 2.1): right-clicking selects
//! the row - deferred, since the table is mid-update here - and offers the
//! panel's registered pod commands, dispatched from the table so each runs
//! exactly as its key does, with that key shown beside it. Check All Rows,
//! last, acts on the whole list.

use super::PodTableDelegate;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::TableState;
use gpui_kit::*;

impl PodTableDelegate {
    pub(super) fn row_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        use crate::k8s::resource::pods::{
            DescribePod, OpenInNewPanel, QuickLook, ShowPodLogs, ShowPodLogsFlipped, ShowPodYaml,
        };
        use crate::ui::bulk::commands::CheckAll;
        use crate::ui::exec_library::commands::{RunFromLibrary, SaveToLibrary};
        // No menu on a deleted pod's row still drawn for its flash (#275).
        if self.live_row(row_ix).is_none() {
            return menu;
        }
        cx.defer_in(window, move |table, _window, cx| {
            table.set_selected_row(row_ix, cx);
        });
        let menu = match &self.action_context {
            Some(focus) => menu.action_context(focus.clone()),
            None => menu,
        };
        menu.menu("Quick Look", Box::new(QuickLook))
            .menu("Open Details", Box::new(DescribePod))
            .menu("Open in New Panel", Box::new(OpenInNewPanel))
            .menu("Logs", Box::new(ShowPodLogs))
            .menu(
                crate::ui::logs_panels::flipped_label(cx),
                Box::new(ShowPodLogsFlipped),
            )
            .menu("YAML", Box::new(ShowPodYaml))
            .menu_with_disabled(
                "Port Forward…",
                Box::new(crate::k8s::resource::pods::PortForwardPod),
                !self.rows[row_ix].row.forwardable(),
            )
            .separator()
            .menu("Run from Library…", Box::new(RunFromLibrary))
            .menu("Save to Library…", Box::new(SaveToLibrary))
            .separator()
            .menu("Check All Rows", Box::new(CheckAll))
    }
}
