//! Rollback and Uninstall as registry commands (`helm-release-actions`'s
//! keyboard routes): Uninstall in the release list and the detail panel,
//! Rollback in the detail panel, where History picks its revision. Each is in
//! the palette while its panel has focus and is overridable in `keymap.toml`.

use crate::command::{Command, CommandRegistry};
use crate::k8s::resource::helm::detail::DETAIL_KEY_CONTEXT;
use crate::k8s::resource::helm::list::LIST_KEY_CONTEXT;
use gpui_kit::{Action, actions};

actions!(helm_actions, [RollbackRelease, UninstallRelease]);

/// Rollback's default key.
pub(crate) const ROLLBACK_KEY: &str = "ctrl-r";
/// Uninstall's default key: the app's delete key, as for any object.
pub(crate) const UNINSTALL_KEY: &str = "ctrl-d";

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, context, action: Box<dyn Action>| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(context),
            action,
            menu: None,
        });
    };
    register(
        "helm.uninstall_release",
        "Helm Releases: Uninstall Release\u{2026}",
        UNINSTALL_KEY,
        LIST_KEY_CONTEXT,
        Box::new(UninstallRelease),
    );
    register(
        "helm_release.rollback",
        "Helm Release: Roll Back\u{2026}",
        ROLLBACK_KEY,
        DETAIL_KEY_CONTEXT,
        Box::new(RollbackRelease),
    );
    register(
        "helm_release.uninstall",
        "Helm Release: Uninstall\u{2026}",
        UNINSTALL_KEY,
        DETAIL_KEY_CONTEXT,
        Box::new(UninstallRelease),
    );
}
