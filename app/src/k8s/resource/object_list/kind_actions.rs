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
    /// What a row's menu offers of its kind's actions: built for that row as
    /// the menu opens, each acting on it.
    pub(super) fn kind_menu(
        &self,
        cx: &Context<Self>,
    ) -> impl Fn(&super::table::ListRow) -> Vec<crate::ui::kind_actions::MenuEntry> + 'static {
        let (kind, context_name, connection) = (
            self.kind.clone(),
            self.scope.context_name.clone(),
            self.connection.clone(),
        );
        let panel = cx.weak_entity();
        move |row| {
            let target = KindTarget {
                context_name: context_name.clone(),
                connection: connection.clone(),
                kind: kind.clone(),
                namespace: row.object.namespace.clone(),
                name: row.object.name.clone(),
                facts: row.object.facts.clone(),
            };
            crate::ui::kind_actions::menu_entries(target, panel.clone())
        }
    }

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
