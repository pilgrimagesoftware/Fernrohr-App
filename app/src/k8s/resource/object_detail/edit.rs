//! Editing the object as YAML (`k9s-remaining-keybindings` section 2): Edit
//! (`e`) swaps the panel's content for an editor over the object's manifest,
//! Save (`cmd-s`) applies it, and Cancel (Escape) drops the edit and shows the
//! object as it was.
//!
//! Save checks the text is a manifest for this object first - a reason, and no
//! request, when it isn't. A refused apply (a stale `resourceVersion`, a field
//! another manager owns) shows the server's reason and keeps the edited text,
//! so the user can fix it and save again. A Secret isn't offered for editing:
//! its values are redacted here, and saving them would overwrite the real ones.

use super::ObjectDetailPanel;
use super::commands::{CancelObjectEdit, EditObject, SaveObjectEdit};
use super::fetch::ObjectDetailState;
use crate::k8s::resource::resource_actions::{self, ActionFailure};
use gpui_kit::component::input::EditorState;
use gpui_kit::*;

/// An edit in progress: the editor, and the last save's refusal, if any.
pub(super) struct YamlEdit {
    pub(super) editor: Entity<EditorState>,
    pub(super) failure: Option<ActionFailure>,
    /// Whether a save is in flight, so a second press can't send it twice.
    pub(super) saving: bool,
}

impl ObjectDetailPanel {
    /// `EditObject`: opens the editor over the object's manifest, focused.
    pub(super) fn on_action_edit(
        &mut self,
        _: &EditObject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit.is_some() {
            return;
        }
        if self.target.kind.gvk.group.is_empty() && self.target.kind.gvk.kind == "Secret" {
            self.edit_notice =
                Some("A Secret can't be edited here: its values are hidden in this view.".into());
            cx.notify();
            return;
        }
        let ObjectDetailState::Loaded(object, _) = &self.state else {
            return;
        };
        let text = resource_actions::edit_text(object);
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("yaml")
                .default_value(text)
        });
        editor.read(cx).focus_handle(cx).focus(window, cx);
        self.edit = Some(YamlEdit {
            editor,
            failure: None,
            saving: false,
        });
        self.edit_notice = None;
        cx.notify();
    }

    /// `CancelObjectEdit`: drops the edit; the object shows as it was.
    pub(super) fn on_action_cancel_edit(
        &mut self,
        _: &CancelObjectEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_edit(window, cx);
    }

    /// The editor binds Escape itself, ahead of the panel's Cancel: while
    /// editing, it cancels the edit instead.
    pub(super) fn capture_editor_escape(
        &mut self,
        _: &gpui_kit::component::input::Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.edit.is_some() {
            self.cancel_edit(window, cx);
            cx.stop_propagation();
        }
    }

    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.take().is_some() {
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    /// `SaveObjectEdit`: applies the edited manifest, if it is one for this
    /// object.
    pub(super) fn on_action_save_edit(
        &mut self,
        _: &SaveObjectEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &mut self.edit else {
            return;
        };
        if edit.saving {
            return;
        }
        let text = edit.editor.read(cx).value().to_string();
        let target = self.target.clone();
        let object = match resource_actions::parse_manifest(
            &text,
            &target.kind,
            &target.name,
            target.namespace.as_deref(),
        ) {
            Ok(object) => object,
            Err(reason) => {
                edit.failure = Some(ActionFailure {
                    message: reason,
                    detail: String::new(),
                });
                cx.notify();
                return;
            }
        };
        let crate::k8s::cluster::connection::ConnectionState::Connected(client) =
            &self.connection.read(cx).state
        else {
            return;
        };
        let client = client.clone();
        edit.saving = true;
        edit.failure = None;
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let result =
                resource_actions::apply(client, &target.kind, target.namespace.as_deref(), object)
                    .await;
            let _ = tx.send(result).await;
        });
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = window_handle.update(cx, |_, window, cx| {
                    let _ = this.update(cx, |this, cx| this.saved(result, window, cx));
                });
            })
            .await;
        })
        .detach();
        cx.notify();
    }

    /// A save landed: done, and the object refetched - or refused, the edit kept.
    fn saved(
        &mut self,
        result: Result<kube::api::DynamicObject, ActionFailure>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(_) => {
                self.cancel_edit(window, cx);
                self.fetch(cx);
            }
            Err(failure) => {
                if let Some(edit) = &mut self.edit {
                    edit.saving = false;
                    edit.failure = Some(failure);
                    cx.notify();
                }
            }
        }
    }

    /// What closing the panel would cost, when it costs something: an edit not
    /// yet saved is lost with it (`panel-move-keybindings`' Close Group asks
    /// first).
    pub fn close_warning(&self) -> Option<String> {
        self.edit.as_ref().map(|_| {
            format!(
                "The unsaved edit to {} {} is lost.",
                self.target.kind.gvk.kind, self.target.name
            )
        })
    }

    /// The editor's current text, for tests.
    #[cfg(test)]
    pub(crate) fn edit_text(&self, cx: &App) -> Option<String> {
        self.edit
            .as_ref()
            .map(|edit| edit.editor.read(cx).value().to_string())
    }
}
