//! `MainWindow`'s handlers for the namespace-set commands
//! (`crate::ui::namespace_sets`): it owns the open panels, so it is what knows
//! which namespaced list has focus, and it re-scopes that list - or, through
//! [`MainWindow::warp_context`], that list's whole context.

use super::panels::PanelKey;
use super::{MainWindow, WindowMode};
use crate::k8s::cluster::namespaces::NamespaceRegistry;
use crate::ui::namespace_sets::editor::{self, EditorMode};
use crate::ui::namespace_sets::picker::{self, Purpose};
use crate::ui::namespace_sets::store::NamespaceSets;
use crate::ui::namespace_sets::{
    ApplyNamespaceSet, CreateNamespaceSet, EditNamespaceSet, RemoveNamespaceSet,
    SwitchContextNamespaceSet, SwitchNamespaceSet,
};
use crate::ui::nav::{self, NavTarget};
use gpui_kit::component::dock::PanelId;
use gpui_kit::*;

impl MainWindow {
    /// The namespaced list with keyboard focus (anywhere inside it): its dock
    /// id and key. `None` when focus is elsewhere, or on a detail, logs or
    /// cluster-scoped panel.
    pub(super) fn focused_namespaced(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<(PanelId, PanelKey)> {
        let WindowMode::Workspace {
            open_panels,
            dock_area,
            ..
        } = &self.mode
        else {
            return None;
        };
        let area = dock_area.read(cx);
        open_panels
            .iter()
            .find(|open| {
                area.panel(open.id)
                    .is_some_and(|view| view.focus_handle(cx).contains_focused(window, cx))
            })
            .filter(|open| matches!(&open.key.target, NavTarget::Kind(kind) if kind.namespaced))
            .map(|open| (open.id, open.key.clone()))
    }

    /// The context a set edit lists namespaces from: the focused list's, or
    /// the window's active one.
    fn set_context(&self, window: &Window, cx: &App) -> Option<String> {
        if let Some((_, key)) = self.focused_namespaced(window, cx) {
            return Some(key.context_name);
        }
        match &self.mode {
            WindowMode::Workspace {
                contexts, active, ..
            } => contexts.get(*active).cloned(),
            WindowMode::Picker(_) => None,
        }
    }

    /// Opens the set editor: on the saved set `name`, or on a new set seeded
    /// with the focused list's namespaces.
    pub(super) fn open_set_editor(
        &mut self,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(context_name) = self.set_context(window, cx) else {
            return;
        };
        let seed = self
            .focused_namespaced(window, cx)
            .map(|(_, key)| key.namespaces)
            .unwrap_or_default();
        let cluster = NamespaceRegistry::list(cx, &context_name);
        let mode = match name {
            Some(name) => EditorMode::Edit { name },
            None => EditorMode::Create,
        };
        editor::open(mode, seed, cluster, window, cx);
    }

    pub(super) fn on_action_create_namespace_set(
        &mut self,
        _: &CreateNamespaceSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_set_editor(None, window, cx);
    }

    fn open_set_picker(&mut self, purpose: Purpose, window: &mut Window, cx: &mut Context<Self>) {
        let has_target = self.focused_namespaced(window, cx).is_some();
        let this = cx.weak_entity();
        picker::open(
            purpose,
            has_target,
            move |name, window, cx| {
                let _ = this.update(cx, |this, cx| this.open_set_editor(Some(name), window, cx));
            },
            window,
            cx,
        );
    }

    pub(super) fn on_action_switch_namespace_set(
        &mut self,
        _: &SwitchNamespaceSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_set_picker(Purpose::Switch, window, cx);
    }

    pub(super) fn on_action_switch_context_namespace_set(
        &mut self,
        _: &SwitchContextNamespaceSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_set_picker(Purpose::SwitchContext, window, cx);
    }

    pub(super) fn on_action_edit_namespace_set(
        &mut self,
        _: &EditNamespaceSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_set_picker(Purpose::Edit, window, cx);
    }

    pub(super) fn on_action_remove_namespace_set(
        &mut self,
        _: &RemoveNamespaceSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_set_picker(Purpose::Remove, window, cx);
    }

    /// Applies a saved set: the focused list takes exactly its namespaces, or
    /// with `context_wide` every namespaced list in that list's context does,
    /// and the context's default becomes the set.
    pub(super) fn on_action_apply_namespace_set(
        &mut self,
        action: &ApplyNamespaceSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(set) = NamespaceSets::get(cx).find(&action.name).cloned() else {
            return;
        };
        let Some((id, key)) = self.focused_namespaced(window, cx) else {
            return;
        };
        if action.context_wide {
            self.warp_context(&key.context_name, set.namespaces, cx);
            return;
        }
        let WindowMode::Workspace {
            open_panels,
            dock_area,
            ..
        } = &self.mode
        else {
            return;
        };
        let opened = open_panels
            .iter()
            .find(|open| open.id == id)
            .and_then(|open| open.panel.clone())
            .or_else(|| nav::opened_panel_for(dock_area.read(cx), id, cx));
        if let Some(opened) = opened
            && opened.set_namespaces(set.namespaces.clone(), cx)
        {
            self.rescope(id, set.namespaces, cx);
        }
    }
}

#[cfg(test)]
mod tests;
