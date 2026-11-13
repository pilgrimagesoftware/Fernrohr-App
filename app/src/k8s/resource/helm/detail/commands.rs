//! The release detail panel's tab keys as registry commands, scoped to the
//! panel's key context (helm-releases D5).

use crate::command::{Command, CommandRegistry};
use gpui_kit::{Action, actions};

actions!(helm_release, [ShowOverview, ShowHistory, ShowNotes]);

/// The panel's key context.
pub(crate) const PANEL_KEY_CONTEXT: &str = "HelmReleasePanel";

/// The tabs' default keys: Values (`3`), Manifest (`4`) and Resources (`6`)
/// join them with their tabs.
pub(super) const OVERVIEW_KEY: &str = "1";
pub(super) const HISTORY_KEY: &str = "2";
pub(super) const NOTES_KEY: &str = "5";

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(PANEL_KEY_CONTEXT),
            action,
            menu: None,
        });
    };
    register(
        "helm_release.overview",
        "Helm Release: Overview",
        OVERVIEW_KEY,
        Box::new(ShowOverview),
    );
    register(
        "helm_release.history",
        "Helm Release: History",
        HISTORY_KEY,
        Box::new(ShowHistory),
    );
    register(
        "helm_release.notes",
        "Helm Release: Notes",
        NOTES_KEY,
        Box::new(ShowNotes),
    );
}
