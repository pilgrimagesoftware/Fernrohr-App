//! The Logs panel's commands: toggling between a container's current logs and
//! its previous instance's - k9s's `p` (`k9s-remaining-keybindings` 5) - and
//! following the logs of the pods a label selector picks (#150), and turning
//! Follow on and off.

use super::labels::WorkloadRef;
use crate::command::{Command, CommandRegistry};
use gpui_kit::{Action, actions};

actions!(
    logs,
    [
        TogglePreviousLogs,
        ToggleFollow,
        ShowLabelLogs,
        FocusLabelSelector,
        NextLabelContainer
    ]
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
/// Added beside [`PANEL_KEY_CONTEXT`] in any Logs panel following a label
/// selector, typed or a workload's, where its container filter's key is bound.
pub const LABEL_LOGS_KEY_CONTEXT: &str = "LabelLogs";
/// The container filter's key: `c` for container.
pub(super) const CONTAINER_FILTER_KEY: &str = "c";
pub(super) const PREVIOUS_KEY: &str = "p";
/// k9s's autoscroll toggle in its log view.
pub(super) const FOLLOW_KEY: &str = "s";
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
    registry.register(Command {
        id: "logs.toggle_follow",
        title: "Logs: Toggle Follow",
        default_binding: FOLLOW_KEY,
        context: Some(PANEL_KEYS_CONTEXT),
        action: Box::new(ToggleFollow),
        menu: None,
    });
    // Global, so it's in the Window menu beside the other ways of opening
    // something, as well as the palette (`selector-logs`).
    registry.register(Command {
        id: "logs.follow_labels",
        title: "Open Logs by Label\u{2026}",
        default_binding: "",
        context: None,
        action: Box::new(ShowLabelLogs),
        menu: Some(crate::command::MenuSlot::Window),
    });
    registry.register(Command {
        id: "logs.next_container",
        title: "Logs: Next Container Filter",
        default_binding: CONTAINER_FILTER_KEY,
        context: Some("LabelLogs && !Input"),
        action: Box::new(NextLabelContainer),
        menu: None,
    });
    // A typed label panel's namespace scope; a workload's is its workload's.
    registry.register(crate::ui::namespace_picker::pick_namespaces_command(
        "logs.pick_namespaces",
        "Logs: Pick Namespaces",
        "TypedLabelLogs && !Input",
    ));
    registry.register(Command {
        id: "logs.focus_selector",
        title: "Logs: Edit Label Selector",
        default_binding: FOCUS_SELECTOR_KEY,
        context: Some("TypedLabelLogs && !Input"),
        action: Box::new(FocusLabelSelector),
        menu: None,
    });
}
