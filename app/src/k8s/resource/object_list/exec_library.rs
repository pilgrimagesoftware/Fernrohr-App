//! A workload's or Service's list in the exec library (`exec-command-library`
//! 4.1, 4.2): Run from Library… and Save to Library… on the selected row, from
//! its menu or the palette. Lists of other kinds offer neither.

use super::panel::ObjectListPanel;
use super::table::ListRow;
use crate::config::exec_library::WorkloadKind;
use crate::ui::exec_library::commands::{RESOURCE_CONTEXT, RunFromLibrary, SaveToLibrary};
use crate::ui::exec_library::resource::{self, Resource};
use crate::ui::kind_actions::MenuEntry;
use gpui_kit::*;
use std::rc::Rc;

impl ObjectListPanel {
    /// The workload kind this list shows, if it can run from the library.
    fn library_kind(&self) -> Option<WorkloadKind> {
        WorkloadKind::of(&self.kind.gvk.group, &self.kind.gvk.kind)
    }

    /// `row` as the library's resource, in a list of `kind`.
    fn library_resource(kind: WorkloadKind, row: &ListRow) -> Resource {
        Resource::Workload {
            kind,
            namespace: row.object.namespace.clone().unwrap_or_default(),
            name: row.object.name.clone(),
            selector: row.object.facts.pod_selector.clone(),
        }
    }

    /// The selected row, as the library's resource.
    fn library_selected(&self, cx: &App) -> Option<(Resource, String)> {
        let kind = self.library_kind()?;
        let row_ix = self.selected_row(cx)?;
        let row = self
            .table
            .as_ref()?
            .read(cx)
            .delegate()
            .rows()
            .get(row_ix)?
            .clone();
        Some((
            Self::library_resource(kind, &row),
            self.scope.context_name.clone(),
        ))
    }

    /// Adds the library's key context, for a list of a kind that can use it.
    pub(super) fn add_exec_library_context(&self, context: &mut KeyContext) {
        if self.library_kind().is_some() {
            context.add(RESOURCE_CONTEXT);
        }
    }

    /// `div` with the library's handlers.
    pub(super) fn on_exec_library_actions(div: Div, cx: &mut Context<Self>) -> Div {
        div.on_action(cx.listener(|this, _: &RunFromLibrary, window, cx| {
            if let Some((resource, context)) = this.library_selected(cx) {
                resource::run_from(resource, context, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &SaveToLibrary, window, cx| {
            if let Some((resource, context)) = this.library_selected(cx) {
                resource::save_from(resource, context, window, cx);
            }
        }))
    }

    /// The row menu's library items, built for `row`: none for a kind that
    /// can't use the library.
    pub(super) fn library_menu(&self) -> impl Fn(&ListRow) -> Vec<MenuEntry> + 'static {
        let (kind, context) = (self.library_kind(), self.scope.context_name.clone());
        move |row| {
            let Some(kind) = kind else {
                return Vec::new();
            };
            let (run, save) = (
                Self::library_resource(kind, row),
                Self::library_resource(kind, row),
            );
            let (run_context, save_context) = (context.clone(), context.clone());
            vec![
                MenuEntry {
                    label: "Run from Library…",
                    action: Box::new(RunFromLibrary),
                    run: Rc::new(move |window, cx| {
                        resource::run_from(run.clone(), run_context.clone(), window, cx)
                    }),
                },
                MenuEntry {
                    label: "Save to Library…",
                    action: Box::new(SaveToLibrary),
                    run: Rc::new(move |window, cx| {
                        resource::save_from(save.clone(), save_context.clone(), window, cx)
                    }),
                },
            ]
        }
    }
}
