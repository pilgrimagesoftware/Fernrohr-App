//! The pod detail panel's part in the exec library (`exec-command-library`
//! 4.1, 4.2): Run from Library… and Save to Library… on its pod, from the
//! palette while the panel has focus.

use super::PodDetailPanel;
use crate::ui::exec_library::commands::{RunFromLibrary, SaveToLibrary};
use crate::ui::exec_library::resource::{self, Resource};
use gpui_kit::*;

impl PodDetailPanel {
    /// The panel's pod, while there is one to act on, and its context.
    fn library_pod(&self) -> Option<(Resource, String)> {
        let pod = self.live_pod()?.clone();
        Some((
            Resource::Pod(Box::new(pod)),
            self.scope.context_name.clone(),
        ))
    }

    /// `div` with the panel's exec library handlers; its context is added with
    /// the panel's own.
    pub(super) fn with_exec_library_actions(div: Div, cx: &mut Context<Self>) -> Div {
        div.on_action(cx.listener(|this, _: &RunFromLibrary, window, cx| {
            if let Some((pod, context)) = this.library_pod() {
                resource::run_from(pod, context, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &SaveToLibrary, window, cx| {
            if let Some((pod, context)) = this.library_pod() {
                resource::save_from(pod, context, window, cx);
            }
        }))
    }
}
