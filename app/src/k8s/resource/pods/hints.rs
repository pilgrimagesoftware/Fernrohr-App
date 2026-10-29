//! The Pods panel's hint row: each shortcut's live key (from the keymap, else
//! its default) and what it does. Shell shows while the selected pod has a
//! running container, the condition that binds `s`; Port forward shows while
//! a pod is selected, since it either forwards or says why it can't; Stop
//! forward while the selected pod has one (`port-forward-indicators` 4.1).

use super::*;
use crate::ui::key_hint_row::{KeyHint, KeyHintRow};
use commands::{LOGS_FLIPPED_KEY, PORT_FORWARD_KEY, SHELL_KEY};
use gpui_kit::prelude::FluentBuilder as _;

/// The debug selector of the hint labelled `label`.
pub(super) fn hint_selector(label: &str) -> String {
    format!("pods-hint {label}")
}

/// One hint: `action`'s live key in the panel, else `fallback`, then `label`.
fn hint(action: &dyn Action, fallback: &str, label: &'static str, window: &Window) -> KeyHint {
    let selector = hint_selector(label);
    KeyHint::styled(
        crate::ui::key_hint_row::live_key(action, Some(PANEL_KEY_CONTEXT), fallback, window),
        label,
        |body| body.debug_selector(move || selector),
    )
}

/// The row's id, which keeps whether it is collapsed.
pub(super) const HINTS_ID: &str = "pods-hints";

impl PodsPanel {
    /// The row, for a selection that `shellable` says can take a shell.
    /// Filter and Describe stay when a narrow panel collapses it
    /// (`key-hint-rows` D2).
    pub(super) fn render_hints(
        &self,
        shellable: bool,
        window: &mut Window,
        cx: &App,
    ) -> KeyHintRow {
        let selected = self.table_selection(cx).is_some();
        let stoppable = !self.selected_forwards(cx).is_empty();
        KeyHintRow::new(HINTS_ID)
            .hint_gap(crate::ui::space::spacing(cx).control_gap)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .hint(hint(&FocusFilter, FILTER_KEY, "Filter", window).essential())
            .hint(hint(&QuickLook, QUICK_LOOK_KEY, "Quick look", window))
            .hint(hint(&WarpNamespace, NAMESPACE_KEY, "Namespace", window))
            .hint(hint(
                &WarpAllToNamespace,
                WARP_ALL_KEY,
                "All panels",
                window,
            ))
            .hint(hint(
                &crate::ui::namespace_picker::PickNamespaces,
                crate::ui::namespace_picker::PICK_NAMESPACES_KEY,
                "Pick namespaces",
                window,
            ))
            .hint(hint(&DescribePod, DESCRIBE_KEY, "Describe", window).essential())
            .hint(hint(
                &OpenInBackground,
                OPEN_IN_BACKGROUND_KEY,
                "Background",
                window,
            ))
            .hint(hint(
                &OpenInNewPanel,
                OPEN_IN_NEW_PANEL_KEY,
                "New panel",
                window,
            ))
            .hint(hint(&ShowPodLogs, LOGS_KEY, "Logs", window))
            .hint(hint(
                &ShowPodLogsFlipped,
                LOGS_FLIPPED_KEY,
                crate::ui::logs_panels::flipped_label(cx),
                window,
            ))
            .hint(hint(&ShowPodYaml, YAML_KEY, "YAML", window))
            .hint(crate::ui::bulk::commands::check_hint(window))
            .hint(crate::ui::bulk::commands::check_all_hint(window))
            .when(shellable, |row| {
                row.hint(hint(&ShellPod, SHELL_KEY, "Shell", window))
            })
            .when(selected, |row| {
                row.hint(hint(
                    &PortForwardPod,
                    PORT_FORWARD_KEY,
                    "Port forward",
                    window,
                ))
            })
            .when(stoppable, |row| {
                row.hint(hint(
                    &StopPortForward,
                    crate::k8s::resource::pods::STOP_PORT_FORWARD_KEY,
                    "Stop forward",
                    window,
                ))
            })
    }
}

#[cfg(test)]
mod tests;
