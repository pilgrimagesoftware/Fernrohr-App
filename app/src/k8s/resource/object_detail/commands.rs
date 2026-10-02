//! The object panel's own action and its registry command. Following a
//! reference (`g`) is `ui::link`'s shared command.

use crate::command::{Command, CommandRegistry};
use gpui_kit::*;

actions!(object_detail, [ToggleObjectView, HideSecretValues]);

/// This panel's own key context - distinct from pod detail's, so the two can
/// bind the same keys to their own panels.
pub const PANEL_KEY_CONTEXT: &str = "ObjectDetailPanel";
pub(super) const TOGGLE_VIEW_KEY: &str = "y";
const TOGGLE_VIEW_COMMAND_ID: &str = "object_detail.toggle_view";
const HIDE_SECRET_VALUES_COMMAND_ID: &str = "object_detail.hide_secret_values";
const HIDE_SECRET_VALUES_KEY: &str = "h";

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
    registry.register(Command {
        id: HIDE_SECRET_VALUES_COMMAND_ID,
        title: "Object Detail: Hide Secret Values",
        default_binding: HIDE_SECRET_VALUES_KEY,
        context: Some(PANEL_KEY_CONTEXT),
        action: Box::new(HideSecretValues),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.copy_name",
        title: "Object Detail: Copy Resource Name",
        default_binding: crate::ui::copy::COPY_NAME_KEY,
        context: Some(PANEL_KEY_CONTEXT),
        action: Box::new(crate::ui::copy::CopyResourceName),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.yaml_fold_all",
        title: "Object Detail: Fold All YAML",
        default_binding: crate::ui::yaml_view::FOLD_ALL_KEY,
        context: Some(PANEL_KEY_CONTEXT),
        action: Box::new(crate::ui::yaml_view::FoldAll),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.yaml_unfold_all",
        title: "Object Detail: Unfold All YAML",
        default_binding: crate::ui::yaml_view::UNFOLD_ALL_KEY,
        context: Some(PANEL_KEY_CONTEXT),
        action: Box::new(crate::ui::yaml_view::UnfoldAll),
        menu: None,
    });
}
