//! The Pods panel's shortcuts as registry commands: the actions, the keys they
//! default to, and their registration.
//!
//! Registered rather than bound directly, so each gets a command-palette entry
//! while a Pods panel is on the focus path, a `keymap.toml` override by id, and -
//! through `keymap::bindings`, which binds every registered command - its key.
//! Owns only the command surface; what each action does lives with the panel's
//! handlers in `pods.rs`.

use crate::command::{Command, CommandRegistry, MenuSlot};
use gpui_kit::{Action, actions};

actions!(pods, [WarpNamespace, DescribePod, ShowPodLogs, ShowPodYaml]);

/// The panel's key context. Every command here is gated to it: `d` means
/// "describe the selected pod" while a Pods panel is on the focus path, and
/// nothing anywhere else. A context binding matches at any depth of that path,
/// so these still fire once a table row has taken focus from the panel.
pub const PANEL_KEY_CONTEXT: &str = "PodsPanel";

/// The default keys, also the hint bar's fallback when the keymap has no
/// binding to show. Named once so the hint bar and the command can't drift.
pub(super) const NAMESPACE_KEY: &str = "w";
pub(super) const DESCRIBE_KEY: &str = "d";
pub(super) const LOGS_KEY: &str = "l";
pub(super) const YAML_KEY: &str = "y";

const NAMESPACE_COMMAND_ID: &str = "pods.warp_namespace";
const DESCRIBE_COMMAND_ID: &str = "pods.describe";
const LOGS_COMMAND_ID: &str = "pods.logs";
const YAML_COMMAND_ID: &str = "pods.yaml";
const FIT_COMMAND_ID: &str = "pods.fit_columns";

/// Registers the panel's shortcuts.
///
/// Menu slots: describe and YAML open a panel, so they sit in Navigate; the
/// namespace warp re-scopes this panel, so View. Logs stays out of the menu -
/// Navigate already has the global "Show Logs", which opens the same selected
/// pod's logs, and two items for one thing is clutter. The native menu greys
/// these out unless a Pods panel is on the focus path.
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
        Some(MenuSlot::View),
    );
    register(
        DESCRIBE_COMMAND_ID,
        "Pods: Describe Selected Pod",
        DESCRIBE_KEY,
        Box::new(DescribePod),
        Some(MenuSlot::Navigate),
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
        Some(MenuSlot::Navigate),
    );
    // The keyboard twin of a double-click on a header divider (`ui::table_fit`).
    register(
        FIT_COMMAND_ID,
        "Pods: Fit Columns to Contents",
        crate::ui::table_fit::FIT_COLUMNS_KEY,
        Box::new(crate::ui::table_fit::FitAllColumns),
        Some(MenuSlot::View),
    );
}

#[cfg(test)]
mod tests;
