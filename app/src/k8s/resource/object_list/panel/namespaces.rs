//! A list panel's namespace scope: narrowing it to the selected object's namespace, to
//! one by position, or to any set - and keeping the per-namespace fallback (#457) in step
//! with it. The picker in the title bar is `render`'s; it scopes through
//! [`ObjectListPanel::set_namespaces`] like everything here.

use super::ObjectListPanel;
use crate::k8s::resource::object_list::commands::WarpNamespace;
use gpui_kit::{Context, Window};

impl ObjectListPanel {
    /// Narrows a namespaced kind's list to the selected object's namespace - the
    /// Pods panel's `w`.
    pub(in crate::k8s::resource::object_list) fn on_action_warp_namespace(
        &mut self,
        _: &WarpNamespace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(namespace) = self
            .selected_row(cx)
            .and_then(|row_ix| {
                self.table
                    .as_ref()?
                    .read(cx)
                    .delegate()
                    .rows()
                    .get(row_ix)
                    .cloned()
            })
            .and_then(|row| row.object.namespace)
        else {
            return;
        };
        self.set_namespaces(vec![namespace], cx);
    }

    /// `JumpToNamespace` (`alt-<n>`): scopes a namespaced list to the namespace at
    /// that position in its namespace list, or to all at 0; past the end, nothing.
    pub(in crate::k8s::resource::object_list) fn on_action_jump_namespace(
        &mut self,
        action: &crate::ui::namespace_jump::JumpToNamespace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.kind.namespaced {
            return;
        }
        let names = self.namespaces.read(cx).names().to_vec();
        if let Some(namespaces) = crate::ui::namespace_jump::scope_for(&names, action.position) {
            self.set_namespaces(namespaces, cx);
        }
    }

    /// Scopes this panel to `namespaces` (empty for all), as its own picker does -
    /// what Warp All to Namespace applies to every namespaced panel in a context.
    pub(crate) fn set_namespaces(&mut self, namespaces: Vec<String>, cx: &mut Context<Self>) {
        self.scope = self.scope.scoped_to(namespaces.clone());
        cx.emit(crate::ui::panel_title::ScopeEvent::NamespacesChanged(
            namespaces,
        ));
        self.sync_fallback(cx);
        cx.notify();
    }
}
