//! A workload's or Service's detail panel in the exec library
//! (`exec-command-library` 4.1, 4.2): Run from Library… and Save to Library…
//! on its object, from the palette while the panel has focus. A panel over any
//! other kind offers neither.

use super::panel::ObjectDetailPanel;
use crate::config::exec_library::WorkloadKind;
use crate::ui::exec_library::commands::{RESOURCE_CONTEXT, RunFromLibrary, SaveToLibrary};
use crate::ui::exec_library::resource::{self, Resource};
use gpui_kit::*;

impl ObjectDetailPanel {
    fn library_kind(&self) -> Option<WorkloadKind> {
        let gvk = &self.target.kind.gvk;
        WorkloadKind::of(&gvk.group, &gvk.kind)
    }

    /// The panel's object as the library's resource, with what it selects
    /// once it has loaded.
    fn library_resource(&self) -> Option<(Resource, String)> {
        let kind = self.library_kind()?;
        let selector = self.object().and_then(|object| {
            crate::k8s::label_selector::of_object(&self.target.kind.gvk, object)
        });
        let resource = Resource::Workload {
            kind,
            namespace: self.target.namespace.clone().unwrap_or_default(),
            name: self.target.name.clone(),
            selector,
        };
        Some((resource, self.scope.context_name.clone()))
    }

    /// Adds the library's key context, for a kind that can use it.
    pub(super) fn add_exec_library_context(&self, context: &mut KeyContext) {
        if self.library_kind().is_some() {
            context.add(RESOURCE_CONTEXT);
        }
    }

    /// `div` with the library's handlers.
    pub(super) fn on_exec_library_actions(div: Div, cx: &mut Context<Self>) -> Div {
        div.on_action(cx.listener(|this, _: &RunFromLibrary, window, cx| {
            if let Some((resource, context)) = this.library_resource() {
                resource::run_from(resource, context, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &SaveToLibrary, window, cx| {
            if let Some((resource, context)) = this.library_resource() {
                resource::save_from(resource, context, window, cx);
            }
        }))
    }
}
