//! The object panel's own action and its registry command. Following a
//! reference (`g`) is `ui::link`'s shared command.

use crate::command::{Command, CommandRegistry};
use gpui_kit::*;

actions!(object_detail, [ToggleObjectView]);

/// This panel's own key context - distinct from pod detail's, so the two can
/// bind the same keys to their own panels.
pub const PANEL_KEY_CONTEXT: &str = "ObjectDetailPanel";
pub(super) const TOGGLE_VIEW_KEY: &str = "y";
const TOGGLE_VIEW_COMMAND_ID: &str = "object_detail.toggle_view";

/// `object_detail.toggle_view`: a palette entry while an object panel has
/// focus, a `keymap.toml` override by id, and its binding (the registry's
/// commands are all bound from the registry). No menu slot - it acts on one
/// panel.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: TOGGLE_VIEW_COMMAND_ID,
        title: "Object Detail: Toggle Fields/YAML",
        default_binding: TOGGLE_VIEW_KEY,
        context: Some(PANEL_KEY_CONTEXT),
        action: Box::new(ToggleObjectView),
        menu: None,
    });
}
