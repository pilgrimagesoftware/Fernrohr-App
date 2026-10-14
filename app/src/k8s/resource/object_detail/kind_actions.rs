//! A detail panel's part in kind-specific actions (`resource-specific-actions`):
//! the shown object as their target - while it is loaded, not known gone and
//! not being edited - its refusal banner for what the cluster refused, and the
//! header's icon buttons and key hints for what the object is offered.

use super::fetch::ObjectDetailState;
use super::panel::ObjectDetailPanel;
use crate::k8s::resource::delete_flow::refusal::Refusal;
use crate::k8s::resource::kind_actions::ObjectFacts;
use crate::ui::detail::lifecycle::Lifecycle;
use crate::ui::kind_actions::{KindHost, KindTarget};
use gpui_kit::*;

impl KindHost for ObjectDetailPanel {
    fn kind_target(&self, cx: &App) -> Option<KindTarget> {
        let gone = matches!(self.lifecycle, Some(Lifecycle::Deleted { .. }));
        if gone || self.edit.is_some() || !matches!(self.state, ObjectDetailState::Loaded(..)) {
            return None;
        }
        let object = self.object()?;
        let mut kind = self.target.kind.clone();
        kind.verbs = self.verbs(cx);
        Some(KindTarget {
            context_name: self.scope.context_name.clone(),
            connection: self.connection.clone(),
            kind,
            namespace: self.target.namespace.clone(),
            name: self.target.name.clone(),
            facts: ObjectFacts::of(object),
        })
    }

    fn show_refusal(&mut self, refusal: Option<Refusal>, cx: &mut Context<Self>) {
        self.refusal = refusal;
        cx.notify();
    }
}

impl ObjectDetailPanel {
    /// The header's mouse and keyboard routes to what the object is offered:
    /// an icon button each, then each one's key hint.
    pub(super) fn kind_header(&self, window: &mut Window, cx: &App) -> Vec<AnyElement> {
        let offered = self
            .kind_target(cx)
            .map(|target| target.offered())
            .unwrap_or_default();
        let mut header = crate::ui::kind_actions::buttons(&offered);
        header.extend(
            crate::ui::kind_actions::hints(&offered, window)
                .into_iter()
                .map(IntoElement::into_any_element),
        );
        header
    }
}

#[cfg(test)]
mod tests;
