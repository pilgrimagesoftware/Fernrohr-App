//! The object panel's own action and its registry command. Following a
//! reference (`g`) is `ui::link`'s shared command.

use crate::command::{Command, CommandRegistry};
use gpui_kit::*;

actions!(
    object_detail,
    [
        ToggleObjectView,
        HideSecretValues,
        EditObject,
        SaveObjectEdit,
        CancelObjectEdit,
        DeleteObject
    ]
);

/// This panel's own key context - distinct from pod detail's, so the two can
/// bind the same keys to their own panels.
pub const PANEL_KEY_CONTEXT: &str = "ObjectDetailPanel";
/// Where the panel's own commands are bound: outside text fields - the YAML
/// editor above all - so `y`, `h` or `e` typed into an edit types.
const PANEL_KEYS_CONTEXT: &str = "ObjectDetailPanel && !Input";
pub(super) const TOGGLE_VIEW_KEY: &str = "y";
const TOGGLE_VIEW_COMMAND_ID: &str = "object_detail.toggle_view";
const HIDE_SECRET_VALUES_COMMAND_ID: &str = "object_detail.hide_secret_values";
const HIDE_SECRET_VALUES_KEY: &str = "h";

/// Added beside [`PANEL_KEY_CONTEXT`] while the object's kind can be patched,
/// so Edit is offered only then.
pub const EDITABLE_KEY_CONTEXT: &str = "EditableObject";
/// Added beside [`PANEL_KEY_CONTEXT`] while the object is being edited, so Save
/// and Cancel's keys mean that only then (`k9s-remaining-keybindings` 2).
pub const EDIT_KEY_CONTEXT: &str = "ObjectYamlEdit";
/// k9s's edit key; Save and Cancel as an editor's.
pub(super) const EDIT_KEY: &str = "e";
pub(super) const SAVE_EDIT_KEY: &str = "cmd-s";
pub(super) const CANCEL_EDIT_KEY: &str = "escape";
/// The lists' delete key, so an object deletes alike wherever it is shown.
pub(super) const DELETE_KEY: &str = "ctrl-d";

/// `object_detail.toggle_view`: a palette entry while an object panel has
/// focus, a `keymap.toml` override by id, and its binding (the registry's
/// commands are all bound from the registry). No menu slot - it acts on one
/// panel.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: TOGGLE_VIEW_COMMAND_ID,
        title: "Object Detail: Toggle Fields/YAML",
        default_binding: TOGGLE_VIEW_KEY,
        context: Some(PANEL_KEYS_CONTEXT),
        action: Box::new(ToggleObjectView),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.edit",
        title: "Object Detail: Edit YAML",
        default_binding: EDIT_KEY,
        // Only for a kind the server lets a client patch.
        context: Some("EditableObject && !Input"),
        action: Box::new(EditObject),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.save_edit",
        title: "Object Detail: Save YAML Edit",
        default_binding: SAVE_EDIT_KEY,
        context: Some(EDIT_KEY_CONTEXT),
        action: Box::new(SaveObjectEdit),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.cancel_edit",
        title: "Object Detail: Cancel YAML Edit",
        default_binding: CANCEL_EDIT_KEY,
        context: Some(EDIT_KEY_CONTEXT),
        action: Box::new(CancelObjectEdit),
        menu: None,
    });
    // Only while the panel's object can be deleted (`delete`).
    registry.register(Command {
        id: "object_detail.delete",
        title: "Object Detail: Delete Object",
        default_binding: DELETE_KEY,
        context: Some("DeletableObject && !Input"),
        action: Box::new(DeleteObject),
        menu: None,
    });
    registry.register(Command {
        id: HIDE_SECRET_VALUES_COMMAND_ID,
        title: "Object Detail: Hide Secret Values",
        default_binding: HIDE_SECRET_VALUES_KEY,
        context: Some(PANEL_KEYS_CONTEXT),
        action: Box::new(HideSecretValues),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.copy_name",
        title: "Object Detail: Copy Resource Name",
        default_binding: crate::ui::copy::COPY_NAME_KEY,
        context: Some(PANEL_KEYS_CONTEXT),
        action: Box::new(crate::ui::copy::CopyResourceName),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.yaml_fold_all",
        title: "Object Detail: Fold All YAML",
        default_binding: crate::ui::yaml_view::FOLD_ALL_KEY,
        context: Some(PANEL_KEYS_CONTEXT),
        action: Box::new(crate::ui::yaml_view::FoldAll),
        menu: None,
    });
    registry.register(Command {
        id: "object_detail.yaml_unfold_all",
        title: "Object Detail: Unfold All YAML",
        default_binding: crate::ui::yaml_view::UNFOLD_ALL_KEY,
        context: Some(PANEL_KEYS_CONTEXT),
        action: Box::new(crate::ui::yaml_view::UnfoldAll),
        menu: None,
    });
}
