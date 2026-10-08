//! Read-only accessors for tests outside this module that embed this table
//! (the Node pods region, #186) and need to assert what it shows without
//! reaching into its private table or watch state. Split from `panel.rs` to
//! keep it under the file-size limit; an inherent `impl` may split across
//! files like any other item (`.claude/rules/rust-structure.md`).

use super::PodsPanel;
use gpui_kit::App;

impl PodsPanel {
    /// The rows currently shown, by name.
    pub(crate) fn test_row_names(&self, cx: &App) -> Vec<String> {
        self.pod_table
            .as_ref()
            .map(|table| {
                table
                    .read(cx)
                    .delegate()
                    .rows()
                    .iter()
                    .map(|row| row.row.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// How many pods the shared watch holds, regardless of this table's own
    /// namespace/node scope or text filter - for a timed-out test elsewhere
    /// in the crate to tell "the watch never delivered" from "it delivered,
    /// but nothing here matched" in its own panic message.
    pub(crate) fn test_watch_pod_count(&self, cx: &App) -> usize {
        self.table.read(cx).pods().len()
    }
}
