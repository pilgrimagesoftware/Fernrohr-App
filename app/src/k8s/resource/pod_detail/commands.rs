//! The pod detail panel's actions, their registry commands, and the
//! keybindings that reach them. Rendering their hints is `render`'s job.

use crate::command::{Command, CommandRegistry};
use gpui_kit::*;

actions!(
    pod_detail,
    [
        ToggleDetailView,
        SelectOverviewTab,
        SelectContainersTab,
        SelectVolumesTab,
        SelectEventsTab,
        SelectManagedFieldsTab
    ]
);

/// This panel's own key context - distinct from `PodsPanel`'s, so the two
/// panels can each bind `y` to a different meaning without conflict (there
/// "open this pod's detail on YAML," here "toggle this already-open panel's
/// view").
pub const PANEL_KEY_CONTEXT: &str = "PodDetailPanel";
pub(super) const TOGGLE_VIEW_KEY: &str = "y";
pub(super) const OVERVIEW_TAB_KEY: &str = "1";
pub(super) const CONTAINERS_TAB_KEY: &str = "2";
pub(super) const VOLUMES_TAB_KEY: &str = "3";
pub(super) const EVENTS_TAB_KEY: &str = "4";
pub(super) const MANAGED_FIELDS_TAB_KEY: &str = "5";

const TOGGLE_VIEW_COMMAND_ID: &str = "pod_detail.toggle_view";
const OVERVIEW_TAB_COMMAND_ID: &str = "pod_detail.tab_overview";
const CONTAINERS_TAB_COMMAND_ID: &str = "pod_detail.tab_containers";
const VOLUMES_TAB_COMMAND_ID: &str = "pod_detail.tab_volumes";
const EVENTS_TAB_COMMAND_ID: &str = "pod_detail.tab_events";
const MANAGED_FIELDS_TAB_COMMAND_ID: &str = "pod_detail.tab_managed_fields";

/// The panel's shortcuts as registry commands, gated to its key context: each
/// gets a palette entry while a detail panel has focus, and a `keymap.toml`
/// override by id. None belongs in the menu bar - they act on one panel, not
/// the app.
pub fn register_commands(registry: &mut CommandRegistry) {
    let commands: [(&'static str, &'static str, &'static str, Box<dyn Action>); 6] = [
        (
            TOGGLE_VIEW_COMMAND_ID,
            "Pod Detail: Toggle Fields/YAML",
            TOGGLE_VIEW_KEY,
            Box::new(ToggleDetailView),
        ),
        (
            OVERVIEW_TAB_COMMAND_ID,
            "Pod Detail: Overview Tab",
            OVERVIEW_TAB_KEY,
            Box::new(SelectOverviewTab),
        ),
        (
            CONTAINERS_TAB_COMMAND_ID,
            "Pod Detail: Containers Tab",
            CONTAINERS_TAB_KEY,
            Box::new(SelectContainersTab),
        ),
        (
            VOLUMES_TAB_COMMAND_ID,
            "Pod Detail: Volumes Tab",
            VOLUMES_TAB_KEY,
            Box::new(SelectVolumesTab),
        ),
        (
            EVENTS_TAB_COMMAND_ID,
            "Pod Detail: Events Tab",
            EVENTS_TAB_KEY,
            Box::new(SelectEventsTab),
        ),
        (
            MANAGED_FIELDS_TAB_COMMAND_ID,
            "Pod Detail: Managed Fields Tab",
            MANAGED_FIELDS_TAB_KEY,
            Box::new(SelectManagedFieldsTab),
        ),
    ];
    for (id, title, default_binding, action) in commands {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(PANEL_KEY_CONTEXT),
            action,
            menu: None,
        });
    }
}
