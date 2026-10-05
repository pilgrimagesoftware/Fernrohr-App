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
        SelectConfigurationTab,
        SelectVolumesTab,
        SelectEventsTab,
        SelectManagedFieldsTab,
        HideSecretValues,
        ViewLogs,
        EditPodYaml
    ]
);

/// This panel's own key context - distinct from `PodsPanel`'s, so the two
/// panels can each bind `y` to a different meaning without conflict (there
/// "open this pod's detail on YAML," here "toggle this already-open panel's
/// view").
pub const PANEL_KEY_CONTEXT: &str = "PodDetailPanel";
pub(super) const TOGGLE_VIEW_KEY: &str = "y";
pub(super) const OVERVIEW_TAB_KEY: &str = "1";
// Tab keys are positional - `pod-configuration-tab` moved Volumes, Events and
// Managed Fields up one each when Configuration took `3`.
pub(super) const CONTAINERS_TAB_KEY: &str = "2";
pub(super) const CONFIGURATION_TAB_KEY: &str = "3";
pub(super) const VOLUMES_TAB_KEY: &str = "4";
pub(super) const EVENTS_TAB_KEY: &str = "5";
pub(super) const MANAGED_FIELDS_TAB_KEY: &str = "6";
pub(super) const HIDE_SECRET_VALUES_KEY: &str = "h";
/// The Pods list's own key for the same intent, so it reads as one shortcut.
pub(super) const VIEW_LOGS_KEY: &str = "l";
/// Where View Logs is bound: the panel minus any text field in it, so its
/// bare letter never fires while typing.
pub(super) const VIEW_LOGS_CONTEXT: &str = "PodDetailPanel && !Input";
/// k9s's edit key, as the object panel's and every list's (#140).
pub(super) const EDIT_KEY: &str = "e";

const TOGGLE_VIEW_COMMAND_ID: &str = "pod_detail.toggle_view";
const OVERVIEW_TAB_COMMAND_ID: &str = "pod_detail.tab_overview";
const CONTAINERS_TAB_COMMAND_ID: &str = "pod_detail.tab_containers";
const CONFIGURATION_TAB_COMMAND_ID: &str = "pod_detail.tab_configuration";
const VOLUMES_TAB_COMMAND_ID: &str = "pod_detail.tab_volumes";
const EVENTS_TAB_COMMAND_ID: &str = "pod_detail.tab_events";
const MANAGED_FIELDS_TAB_COMMAND_ID: &str = "pod_detail.tab_managed_fields";
const HIDE_SECRET_VALUES_COMMAND_ID: &str = "pod_detail.hide_secret_values";
const FOLD_ALL_COMMAND_ID: &str = "pod_detail.yaml_fold_all";
const COPY_NAME_COMMAND_ID: &str = "pod_detail.copy_name";
const UNFOLD_ALL_COMMAND_ID: &str = "pod_detail.yaml_unfold_all";
const VIEW_LOGS_COMMAND_ID: &str = "pod_detail.view_logs";

/// The panel's shortcuts as registry commands, gated to its key context: each
/// gets a palette entry while a detail panel has focus, and a `keymap.toml`
/// override by id. None belongs in the menu bar - they act on one panel, not
/// the app.
pub fn register_commands(registry: &mut CommandRegistry) {
    let commands: [(&'static str, &'static str, &'static str, Box<dyn Action>); 11] = [
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
            CONFIGURATION_TAB_COMMAND_ID,
            "Pod Detail: Configuration Tab",
            CONFIGURATION_TAB_KEY,
            Box::new(SelectConfigurationTab),
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
        (
            HIDE_SECRET_VALUES_COMMAND_ID,
            "Pod Detail: Hide Secret Values",
            HIDE_SECRET_VALUES_KEY,
            Box::new(HideSecretValues),
        ),
        (
            FOLD_ALL_COMMAND_ID,
            "Pod Detail: Fold All YAML",
            crate::ui::yaml_view::FOLD_ALL_KEY,
            Box::new(crate::ui::yaml_view::FoldAll),
        ),
        (
            UNFOLD_ALL_COMMAND_ID,
            "Pod Detail: Unfold All YAML",
            crate::ui::yaml_view::UNFOLD_ALL_KEY,
            Box::new(crate::ui::yaml_view::UnfoldAll),
        ),
        (
            COPY_NAME_COMMAND_ID,
            "Pod Detail: Copy Resource Name",
            crate::ui::copy::COPY_NAME_KEY,
            Box::new(crate::ui::copy::CopyResourceName),
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
    registry.register(Command {
        id: VIEW_LOGS_COMMAND_ID,
        title: "Pod Detail: View Logs",
        default_binding: VIEW_LOGS_KEY,
        context: Some(VIEW_LOGS_CONTEXT),
        action: Box::new(ViewLogs),
        menu: None,
    });
    registry.register(Command {
        id: "pod_detail.edit",
        title: "Pod Detail: Edit YAML",
        default_binding: EDIT_KEY,
        context: Some(VIEW_LOGS_CONTEXT),
        action: Box::new(EditPodYaml),
        menu: None,
    });
    super::window_commands::register_commands(registry);
}
