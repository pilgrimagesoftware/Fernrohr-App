//! The Pods list's part in the exec library (`exec-command-library` 4.1, 4.2):
//! Run from Library… and Save to Library… on the selected pod, from its row's
//! menu or the palette.

use super::PodsPanel;
use crate::ui::exec_library::commands::{RESOURCE_CONTEXT, RunFromLibrary, SaveToLibrary};
use crate::ui::exec_library::resource::{self, Resource};
use gpui_kit::*;

impl PodsPanel {
    /// The selected pod, as the watch has it, and its context.
    fn library_pod(&self, cx: &App) -> Option<(Resource, String)> {
        let selection = self.table_selection(cx)?;
        let pod = self
            .table
            .read(cx)
            .find(&selection.namespace, &selection.name, None)?
            .clone();
        Some((
            Resource::Pod(Box::new(pod)),
            self.scope.context_name.clone(),
        ))
    }

    /// `div` with the panel's exec library context and handlers.
    pub(super) fn on_exec_library_actions(
        div: Div,
        key_context: &mut KeyContext,
        cx: &mut Context<Self>,
    ) -> Div {
        key_context.add(RESOURCE_CONTEXT);
        div.on_action(cx.listener(|this, _: &RunFromLibrary, window, cx| {
            if let Some((pod, context)) = this.library_pod(cx) {
                resource::run_from(pod, context, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &SaveToLibrary, window, cx| {
            if let Some((pod, context)) = this.library_pod(cx) {
                resource::save_from(pod, context, window, cx);
            }
        }))
    }
}
