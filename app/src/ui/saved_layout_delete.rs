//! The one confirmation deleting a saved layout goes through
//! (`saved-panel-layouts` design.md D6): shared by the saved layouts picker's
//! `saved_layouts.delete_selected` (`backspace`, `ui::picker::saved_layouts`)
//! and the Settings window's Layouts section's per-row Remove control
//! (`ui::settings::layouts`), so deleting reads, looks and keys alike from
//! either surface - the same `Severity::Irreversible` tier
//! `ui/tunnels/editor/actions.rs::request_delete` and
//! `ui::forward_stop::confirm_stop` already use for their own destructive
//! confirmations: focus opens on Cancel, so a bare Enter declines, and the
//! danger-styled Delete button runs only on click, Tab+Enter/Space, or the
//! `dialog.confirm_irreversible` shortcut.

use crate::config::saved_layouts;
use crate::ui::confirm_dialog::{self, Confirmation, Severity};
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::*;
use std::path::PathBuf;

/// Asks before deleting the saved layout named `layout_name` under `dir`,
/// removing it via [`saved_layouts::remove`] only once the user confirms
/// deliberately; `on_removed` runs afterward, so each caller can refresh its
/// own list (the picker's `reload`, the Settings section's next render).
/// `id_prefix` namespaces the dialog's own Cancel/Delete button ids
/// ([`confirm_dialog::open`]'s convention), so the two call sites' buttons
/// never collide.
pub(crate) fn confirm_delete(
    layout_name: &str,
    id_prefix: &'static str,
    dir: PathBuf,
    on_removed: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let confirmation = Confirmation {
        title: "Delete Saved Layout?".into(),
        body: ConfirmText::new()
            .text("Deleting ")
            .name(layout_name)
            .text(" can't be undone."),
        confirm: "Delete".into(),
        id_prefix,
        severity: Severity::Irreversible,
    };
    let name = layout_name.to_string();
    confirm_dialog::open(
        confirmation,
        move |window, cx| {
            if let Err(error) = saved_layouts::remove(&dir, &name) {
                log::warn!("failed to remove saved layout {name:?}: {error}");
            }
            crate::util::shell::note_layouts_changed(cx);
            on_removed(window, cx);
        },
        window,
        cx,
    );
}
