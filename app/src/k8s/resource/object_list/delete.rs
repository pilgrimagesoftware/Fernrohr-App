//! Delete the selected object from any kind's list (`k9s-remaining-keybindings`):
//! `ctrl-d` asks, naming the object, and deletes it through `delete_flow` - the
//! Pods list's question and request, for Secrets, ConfigMaps, Deployments and
//! the rest.
//!
//! Offered only for a kind discovery lists the `delete` verb for: the table
//! sits in a [`DELETABLE_KEY_CONTEXT`] element then, which is the command's
//! context, so neither the key nor the palette offers it otherwise. A delete
//! that lands needs nothing here - the watch drops the row; a refused one
//! shows why above the table.

use super::commands::DeleteSelected;
use super::panel::ObjectListPanel;
use crate::k8s::resource::delete_flow::refusal::{self, Refusal};
use crate::k8s::resource::delete_flow::{self, DeleteTarget};
use gpui_kit::*;

/// The context the list's Delete command lives in.
pub const DELETABLE_KEY_CONTEXT: &str = "DeletableList";
/// The refusal banner, and its Dismiss button.
pub(super) const REFUSAL_ID: &str = "object-list-refusal";
pub(super) const DISMISS_REFUSAL_ID: &str = "object-list-refusal-dismiss";

impl ObjectListPanel {
    /// Whether this list's kind can be deleted, as discovery reported it.
    pub(super) fn deletable(&self) -> bool {
        self.kind.verbs.delete
    }

    /// The selected row's object, as a delete addresses it.
    fn delete_target(&self, cx: &App) -> Option<DeleteTarget> {
        let row_ix = self.selected_row(cx)?;
        let table = self.table.as_ref()?.read(cx);
        let object = &table.delegate().rows().get(row_ix)?.object;
        Some(DeleteTarget {
            context_name: self.scope.context_name.clone(),
            kind: self.kind.clone(),
            namespace: object.namespace.clone(),
            name: object.name.clone(),
        })
    }

    /// `DeleteSelected`: asks before deleting the selected object.
    pub(super) fn on_action_delete_selected(
        &mut self,
        _: &DeleteSelected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.deletable() {
            return;
        }
        let Some(target) = self.delete_target(cx) else {
            return;
        };
        self.refusal = None;
        cx.notify();
        let panel = cx.weak_entity();
        let action = target.action(false);
        let on_done: delete_flow::OnDeleted = std::rc::Rc::new(move |result, cx| {
            let _ = panel.update(cx, |panel, cx| {
                panel.refusal = result.err().map(|failure| Refusal {
                    action: action.clone(),
                    failure,
                });
                cx.notify();
            });
        });
        let connection = self.connection.clone();
        delete_flow::confirm_delete(target, connection, on_done, window, cx);
    }

    /// The banner for a refused delete, above the table.
    pub(super) fn render_refusal(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let refused = self.refusal.clone()?;
        let this = cx.weak_entity();
        Some(refusal::render(
            refused,
            REFUSAL_ID,
            DISMISS_REFUSAL_ID,
            move |cx| {
                let _ = this.update(cx, |this, cx| {
                    this.refusal = None;
                    cx.notify();
                });
            },
            cx,
        ))
    }
}

#[cfg(test)]
mod tests;
