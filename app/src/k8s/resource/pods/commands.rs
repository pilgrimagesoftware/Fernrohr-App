//! The Pods panel's shortcuts as registry commands: the actions, the keys they
//! default to, and their registration.
//!
//! Registered rather than bound directly, so each gets a command-palette entry
//! while a Pods panel is on the focus path, a `keymap.toml` override by id, and -
//! through `keymap::bindings`, which binds every registered command - its key.
//! Owns only the command surface; what each action does lives with the panel's
//! handlers in `pods.rs`.

use crate::command::{Command, CommandRegistry};
use gpui_kit::{Action, actions};

actions!(
    pods,
    [
        WarpNamespace,
        WarpAllToNamespace,
        DescribePod,
        ShowPodLogs,
        ShowPodYaml
    ]
);

/// The panel's key context. Every command here is gated to it: `d` means
/// "describe the selected pod" while a Pods panel is on the focus path, and
/// nothing anywhere else. A context binding matches at any depth of that path,
/// so these still fire once a table row has taken focus from the panel.
pub const PANEL_KEY_CONTEXT: &str = "PodsPanel";

/// The default keys, also the hint bar's fallback when the keymap has no
/// binding to show. Named once so the hint bar and the command can't drift.
pub(super) const NAMESPACE_KEY: &str = "w";
/// Warp All to Namespace (`warp-all-to-namespace`): `w` for this panel alone,
/// shifted for every list in the context.
pub(super) const WARP_ALL_KEY: &str = "shift-w";
pub(super) const DESCRIBE_KEY: &str = "d";
pub(super) const LOGS_KEY: &str = "l";
pub(super) const YAML_KEY: &str = "y";

const NAMESPACE_COMMAND_ID: &str = "pods.warp_namespace";
const WARP_ALL_COMMAND_ID: &str = "pods.warp_all_namespace";
const DESCRIBE_COMMAND_ID: &str = "pods.describe";
const LOGS_COMMAND_ID: &str = "pods.logs";
const YAML_COMMAND_ID: &str = "pods.yaml";
const FIT_COMMAND_ID: &str = "pods.fit_columns";

/// Registers the panel's shortcuts.
///
/// None is in the menu bar: they act only in a Pods panel, and the menu bar
/// holds global commands only, so it never depends on focus
/// (`menu-organization`). The palette offers them while a Pods panel is on the
/// focus path, and the hint row shows their keys.
pub fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>, menu| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(PANEL_KEY_CONTEXT),
            action,
            menu,
        });
    };
    register(
        NAMESPACE_COMMAND_ID,
        "Pods: Filter to Selected Pod's Namespace",
        NAMESPACE_KEY,
        Box::new(WarpNamespace),
        None,
    );
    register(
        WARP_ALL_COMMAND_ID,
        "Pods: Warp All to Selected Pod's Namespace",
        WARP_ALL_KEY,
        Box::new(WarpAllToNamespace),
        None,
    );
    register(
        DESCRIBE_COMMAND_ID,
        "Pods: Describe Selected Pod",
        DESCRIBE_KEY,
        Box::new(DescribePod),
        None,
    );
    register(
        LOGS_COMMAND_ID,
        "Pods: Show Selected Pod's Logs",
        LOGS_KEY,
        Box::new(ShowPodLogs),
        None,
    );
    register(
        YAML_COMMAND_ID,
        "Pods: Show Selected Pod's YAML",
        YAML_KEY,
        Box::new(ShowPodYaml),
        None,
    );
    // The keyboard twin of a double-click on a header divider (`ui::table_fit`).
    register(
        FIT_COMMAND_ID,
        "Pods: Fit Columns to Contents",
        crate::ui::table_fit::FIT_COLUMNS_KEY,
        Box::new(crate::ui::table_fit::FitAllColumns),
        None,
    );
    registry.register(crate::ui::namespace_picker::pick_namespaces_command(
        "pods.pick_namespaces",
        "Pods: Pick Namespaces",
        PANEL_KEY_CONTEXT,
    ));
}

#[cfg(test)]
mod tests;
