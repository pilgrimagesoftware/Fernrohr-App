//! The Logs panel's commands: toggling between a container's current logs and
//! its previous instance's - k9s's `p` (`k9s-remaining-keybindings` 5) - and
//! following the logs of the pods a label selector picks (#150).

use super::labels::WorkloadRef;
use crate::command::{Command, CommandRegistry};
use gpui_kit::{Action, actions};

actions!(
    logs,
    [TogglePreviousLogs, ShowLabelLogs, FocusLabelSelector]
);

/// Opens (or focuses) a Logs panel following `workload`'s pods in
/// `context_name` - what `l` in a workload's detail panel dispatches. Carries
/// its workload, like `object_list::OpenListedObject`, so the window's one
/// open path is all it needs.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = logs, no_json)]
pub struct OpenWorkloadLogs {
    pub context_name: String,
    pub workload: WorkloadRef,
}

/// The panel's key context: its command means something only while it has focus.
pub const PANEL_KEY_CONTEXT: &str = "LogsPanel";
/// Where the panel's own keys are bound: outside a typed panel's selector
/// field, so `p` typed into a selector types.
const PANEL_KEYS_CONTEXT: &str = "LogsPanel && !Input";
/// Added beside [`PANEL_KEY_CONTEXT`] in a Logs panel whose selector is typed
/// into it, so the key that focuses the field means that only there.
pub const TYPED_LABELS_KEY_CONTEXT: &str = "TypedLabelLogs";
pub(super) const PREVIOUS_KEY: &str = "p";
/// The lists' filter key, for the field a selector is typed into.
pub(super) const FOCUS_SELECTOR_KEY: &str = "/";

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: "logs.toggle_previous",
        title: "Logs: Toggle Previous Container Logs",
        default_binding: PREVIOUS_KEY,
        context: Some(PANEL_KEYS_CONTEXT),
        action: Box::new(TogglePreviousLogs),
        menu: None,
    });
    // Palette and keymap only, like Show Events.
    registry.register(Command {
        id: "logs.follow_labels",
        title: "Logs: Follow Labels\u{2026}",
        default_binding: "",
        context: None,
        action: Box::new(ShowLabelLogs),
        menu: None,
    });
    registry.register(Command {
        id: "logs.focus_selector",
        title: "Logs: Edit Label Selector",
        default_binding: FOCUS_SELECTOR_KEY,
        context: Some("TypedLabelLogs && !Input"),
        action: Box::new(FocusLabelSelector),
        menu: None,
    });
}
