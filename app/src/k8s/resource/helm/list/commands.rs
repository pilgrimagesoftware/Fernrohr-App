//! The Helm Releases list's shortcuts as registry commands, scoped to the
//! panel's key context. What each does lives with the panel's handlers.

use crate::command::{Command, CommandRegistry};
use gpui_kit::{Action, actions};

actions!(helm_list, [FocusSearch, ToggleUninstalled]);

/// The panel's key context.
pub(crate) const PANEL_KEY_CONTEXT: &str = "HelmReleasesPanel";
/// [`PANEL_KEY_CONTEXT`] outside the search box, which needs these keys typed.
const KEY_CONTEXT: &str = "HelmReleasesPanel && !Input";

/// The default keys, also the hint row's fallback when the keymap has none.
pub(super) const SEARCH_KEY: &str = "/";

/// Registers the panel's commands. None is in the menu bar, like every
/// panel-scoped command.
pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(KEY_CONTEXT),
            action,
            menu: None,
        });
    };
    register(
        "helm.focus_search",
        "Helm Releases: Search",
        SEARCH_KEY,
        Box::new(FocusSearch),
    );
    // No default key: an occasional toggle, from the palette or the hint row.
    register(
        "helm.show_uninstalled",
        "Helm Releases: Show Uninstalled Releases",
        "",
        Box::new(ToggleUninstalled),
    );
}
