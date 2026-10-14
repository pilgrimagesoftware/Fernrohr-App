//! A list panel's part in kind-specific actions (`resource-specific-actions`):
//! the selected row's object as their target, and the panel's refusal banner
//! for what the cluster refused.

use super::panel::ObjectListPanel;
use crate::k8s::resource::delete_flow::refusal::Refusal;
use crate::ui::kind_actions::{KindHost, KindTarget};
use gpui_kit::*;

impl KindHost for ObjectListPanel {
    fn kind_target(&self, cx: &App) -> Option<KindTarget> {
        let row_ix = self.selected_row(cx)?;
        let row = self
            .table
            .as_ref()?
            .read(cx)
            .delegate()
            .rows()
            .get(row_ix)?
            .object
            .clone();
        Some(KindTarget {
            context_name: self.scope.context_name.clone(),
            connection: self.connection.clone(),
            kind: self.kind.clone(),
            namespace: row.namespace,
            name: row.name,
            facts: row.facts,
        })
    }

    fn show_refusal(&mut self, refusal: Option<Refusal>, cx: &mut Context<Self>) {
        self.refusal = refusal;
        cx.notify();
    }
}

impl ObjectListPanel {
    /// The hint row's entries for what the selected row is offered.
    pub(super) fn kind_hints(&self, window: &mut Window, cx: &App) -> Vec<Div> {
        let offered = self
            .kind_target(cx)
            .map(|target| target.offered())
            .unwrap_or_default();
        crate::ui::kind_actions::hints(&offered, window)
    }
}

#[cfg(test)]
mod tests;
