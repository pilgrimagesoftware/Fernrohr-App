//! The tunnel editor as a modal dialog over the Tunnels window (Fernrohr#165): the
//! editor's fields as the dialog's body, which scrolls when taller than the window,
//! under a title, with Delete (editing only), Cancel and Save in a footer that
//! stays in view.
//!
//! Focus starts on the first field. Enter saves from any single-line field - an
//! input passes Enter on to the dialog - while the multi-line command keeps Enter
//! for its newlines; Escape cancels. Save and Cancel show those keys. A failed
//! save keeps the dialog open with its errors inline; a save, delete or cancel
//! closes it, through the one subscription in [`TunnelsWindow::open_editor`], and
//! focus goes back to where it was when the dialog opened - the edited tunnel's row.

use super::*;
use crate::ui::confirm_dialog;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::dialog::DialogFooter;

/// The footer buttons' ids.
pub(super) const SAVE_ID: &str = "tunnel-save";
pub(super) const CANCEL_ID: &str = "tunnel-cancel";
pub(super) const DELETE_ID: &str = "tunnel-delete";

/// The dialog's width: room for the command line and the mode help beside it.
const DIALOG_WIDTH: Pixels = px(560.);

impl TunnelsWindow {
    /// Opens `editor` in its dialog and closes the dialog once it reports.
    pub(super) fn open_editor(
        &mut self,
        editor: Entity<TunnelEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(&editor, window, |this, _editor, event, window, cx| {
            window.close_dialog(cx);
            this.editor = None;
            match event {
                TunnelEditorEvent::Saved | TunnelEditorEvent::Deleted => this.refresh(cx),
                TunnelEditorEvent::Cancelled => cx.notify(),
            }
        })
        .detach();
        self.editor = Some(editor.clone());

        let title = if editor.read(cx).is_new() {
            "New Tunnel"
        } else {
            "Edit Tunnel"
        };
        let body = editor.clone();
        window.open_dialog(cx, move |dialog, window, cx| {
            let editor = &body;
            let (ok, cancel) = (editor.clone(), editor.clone());
            dialog
                .title(title)
                .w(DIALOG_WIDTH)
                // `content`, not `child`: the editor scrolls itself, following
                // focus, where the dialog's own scrolling body would not.
                .content({
                    let editor = editor.clone();
                    move |content, _window, _cx| content.flex_1().min_h_0().child(editor.clone())
                })
                // Each closes the dialog only through the editor's event, so a
                // save that fails validation stays open with its errors.
                .on_ok(move |_event, window, cx| {
                    ok.update(cx, |editor, cx| editor.save(window, cx));
                    false
                })
                .on_cancel(move |_event, _window, cx| {
                    cancel.update(cx, |editor, cx| editor.cancel(cx));
                    false
                })
                .footer(footer(editor, window, cx))
        });
        editor.update(cx, |editor, cx| editor.focus_first_field(window, cx));
    }
}

/// Delete (only for an existing tunnel) on the left; Cancel and Save, each showing
/// its key, on the right.
fn footer(editor: &Entity<TunnelEditor>, window: &mut Window, cx: &App) -> DialogFooter {
    let (save, cancel, delete) = (editor.clone(), editor.clone(), editor.clone());
    let delete = (!editor.read(cx).is_new()).then(|| {
        Button::new(DELETE_ID)
            .label("Delete")
            .danger()
            .on_click(move |_event, window, cx| {
                delete.update(cx, |editor, cx| editor.request_delete(window, cx));
            })
    });
    DialogFooter::new()
        .children(delete)
        .child(div().flex_1())
        .child(
            Button::new(CANCEL_ID)
                .label("Cancel")
                .child(confirm_dialog::cancel_key(window))
                .on_click(move |_event, _window, cx| {
                    cancel.update(cx, |editor, cx| editor.cancel(cx));
                }),
        )
        .child(
            Button::new(SAVE_ID)
                .label("Save")
                .primary()
                .child(confirm_dialog::confirm_key(window))
                .on_click(move |_event, window, cx| {
                    save.update(cx, |editor, cx| editor.save(window, cx));
                }),
        )
}

#[cfg(test)]
mod tests;
