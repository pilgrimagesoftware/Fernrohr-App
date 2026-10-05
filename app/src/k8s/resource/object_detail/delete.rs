//! Delete the shown object (`k9s-remaining-keybindings`): `ctrl-d` asks, naming
//! it, and deletes it through `delete_flow` - the lists' question and request.
//!
//! Offered only while there is an object to delete - loaded, not already gone -
//! and only for a kind discovery lists the `delete` verb for: the panel then
//! carries [`DELETABLE_KEY_CONTEXT`], the command's context. A delete that
//! lands needs nothing here: the panel follows its object (`live`), so it shows
//! the object Terminating or deleted as the cluster reports it. A refused one
//! shows why above the content.

use super::commands::DeleteObject;
use super::fetch::ObjectDetailState;
use super::panel::ObjectDetailPanel;
use crate::k8s::cluster::discovery::KindVerbs;
use crate::k8s::resource::delete_flow::refusal::{self, Refusal};
use crate::k8s::resource::delete_flow::{self, DeleteTarget};
use crate::ui::detail::lifecycle::Lifecycle;
use gpui_kit::*;

/// Added beside the panel's own context while its object can be deleted.
pub const DELETABLE_KEY_CONTEXT: &str = "DeletableObject";
/// The refusal banner, and its Dismiss button.
pub(super) const REFUSAL_ID: &str = "object-detail-refusal";
pub(super) const DISMISS_REFUSAL_ID: &str = "object-detail-refusal-dismiss";

impl ObjectDetailPanel {
    /// What the server lets a client do with the object's kind - by
    /// discovery's word when it has one, else the kind the panel was opened
    /// with (a restored panel's assumes everything). Gates Delete here and
    /// Edit (`edit`).
    pub(super) fn verbs(&self, cx: &App) -> KindVerbs {
        self.discovery
            .read(cx)
            .kinds()
            .and_then(|kinds| kinds.iter().find(|kind| **kind == self.target.kind))
            .map_or(self.target.kind.verbs, |kind| kind.verbs)
    }

    /// Whether Delete is offered: the object is loaded and not known gone, its
    /// kind can be deleted, and no YAML edit is open - deleting under an edit
    /// would silently discard it, even with focus on its Save or Cancel
    /// button rather than in the editor.
    pub(super) fn deletable(&self, cx: &App) -> bool {
        let loaded = matches!(self.state, ObjectDetailState::Loaded(..));
        let gone = matches!(self.lifecycle, Some(Lifecycle::Deleted { .. }));
        loaded && !gone && self.edit.is_none() && self.verbs(cx).delete
    }

    /// `DeleteObject`: asks before deleting the shown object.
    pub(super) fn on_action_delete_object(
        &mut self,
        _: &DeleteObject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.deletable(cx) {
            return;
        }
        let target = DeleteTarget {
            context_name: self.scope.context_name.clone(),
            kind: self.target.kind.clone(),
            namespace: self.target.namespace.clone(),
            name: self.target.name.clone(),
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

    /// The banner for a refused delete, above the content.
    pub(super) fn render_refusal(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let refused = self.refusal.clone()?;
        let this = cx.weak_entity();
        let banner = refusal::render(
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
        );
        let space = crate::ui::space::spacing(cx);
        Some(
            div()
                .px(space.panel_inset)
                .pt(space.control_gap)
                .child(banner)
                .into_any_element(),
        )
    }
}
