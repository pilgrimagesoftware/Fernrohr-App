//! The Helm Releases list's shortcuts as registry commands, scoped to the
//! panel's key context. What each does lives with the panel's handlers.

use crate::command::{Command, CommandRegistry};
use crate::k8s::resource::helm::ReleaseKey;
use gpui_kit::{Action, actions};

actions!(
    helm_list,
    [FocusSearch, ToggleUninstalled, OpenRelease, DescribeRelease]
);

/// Opens (or focuses) one release's detail panel in `context_name`: what a
/// row's Enter, `d` and double-click ask the window for.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = helm_list, no_json)]
pub(crate) struct OpenHelmRelease {
    pub(crate) context_name: String,
    pub(crate) key: ReleaseKey,
}

/// The panel's key context.
pub(crate) const PANEL_KEY_CONTEXT: &str = "HelmReleasesPanel";
/// [`PANEL_KEY_CONTEXT`] outside the search box, which needs these keys typed
/// - also where Up and Down move the list's selection (`list_keys`).
pub(crate) const KEY_CONTEXT: &str = "HelmReleasesPanel && !Input";

/// The default keys, also the hint row's fallback when the keymap has none.
pub(super) const SEARCH_KEY: &str = "/";
pub(super) const OPEN_KEY: &str = "enter";
pub(super) const DESCRIBE_KEY: &str = "d";

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
    register(
        "helm.open_release",
        "Helm Releases: Open Release",
        OPEN_KEY,
        Box::new(OpenRelease),
    );
    // `d`, describe, as every list binds it.
    register(
        "helm.describe_release",
        "Helm Releases: Describe Release",
        DESCRIBE_KEY,
        Box::new(DescribeRelease),
    );
    // No default key: an occasional toggle, from the palette or the hint row.
    register(
        "helm.show_uninstalled",
        "Helm Releases: Show Uninstalled Releases",
        "",
        Box::new(ToggleUninstalled),
    );
}
