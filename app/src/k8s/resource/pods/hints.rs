//! The Pods panel's hint row: each shortcut's live key (from the keymap, else
//! its default) and what it does. Shell shows while the selected pod has a
//! running container, the condition that binds `s`; Port forward shows while
//! a pod is selected, since it either forwards or says why it can't; Stop
//! forward while the selected pod has one (`port-forward-indicators` 4.1).

use super::*;
use crate::ui::list_search::ListSearch;
use commands::{LOGS_FLIPPED_KEY, PORT_FORWARD_KEY, SHELL_KEY};
use gpui_kit::prelude::FluentBuilder as _;

/// The debug selector of the hint labelled `label`.
pub(super) fn hint_selector(label: &str) -> String {
    format!("pods-hint {label}")
}

/// One hint: `action`'s live key in the panel, else `fallback`, then `label`.
fn hint(action: &dyn Action, fallback: &str, label: &'static str, window: &mut Window) -> Div {
    let selector = hint_selector(label);
    ListSearch::hint(action, PANEL_KEY_CONTEXT, fallback, label, window)
        .debug_selector(move || selector)
}

impl PodsPanel {
    /// The row, for a selection that `shellable` says can take a shell.
    pub(super) fn render_hints(&self, shellable: bool, window: &mut Window, cx: &App) -> Div {
        let selected = self.table_selection(cx).is_some();
        let stoppable = !self.selected_forwards(cx).is_empty();
        div()
            .flex()
            .flex_wrap()
            .gap(crate::ui::space::spacing(cx).control_gap)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(hint(&FocusFilter, FILTER_KEY, "Filter", window))
            .child(hint(&QuickLook, QUICK_LOOK_KEY, "Quick look", window))
            .child(hint(&WarpNamespace, NAMESPACE_KEY, "Namespace", window))
            .child(hint(
                &WarpAllToNamespace,
                WARP_ALL_KEY,
                "All panels",
                window,
            ))
            .child(hint(
                &crate::ui::namespace_picker::PickNamespaces,
                crate::ui::namespace_picker::PICK_NAMESPACES_KEY,
                "Pick namespaces",
                window,
            ))
            .child(hint(&DescribePod, DESCRIBE_KEY, "Describe", window))
            .child(hint(
                &OpenInBackground,
                OPEN_IN_BACKGROUND_KEY,
                "Background",
                window,
            ))
            .child(hint(&ShowPodLogs, LOGS_KEY, "Logs", window))
            .child(hint(
                &ShowPodLogsFlipped,
                LOGS_FLIPPED_KEY,
                crate::ui::logs_panels::flipped_label(cx),
                window,
            ))
            .child(hint(&ShowPodYaml, YAML_KEY, "YAML", window))
            .when(shellable, |row| {
                row.child(hint(&ShellPod, SHELL_KEY, "Shell", window))
            })
            .when(selected, |row| {
                row.child(hint(
                    &PortForwardPod,
                    PORT_FORWARD_KEY,
                    "Port forward",
                    window,
                ))
            })
            .when(stoppable, |row| {
                row.child(hint(
                    &StopPortForward,
                    crate::k8s::resource::pods::STOP_PORT_FORWARD_KEY,
                    "Stop forward",
                    window,
                ))
            })
    }
}
