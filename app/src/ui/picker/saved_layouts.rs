//! The saved layouts picker (`saved-panel-layouts` tasks section 3): lists
//! every [`crate::config::saved_layouts::SavedLayout`] by name, and lets the
//! user rename or delete the selected one. `layouts.manage` - the window-level
//! command that opens this as a dialog over a live `MainWindow`, in both
//! `Workspace` and cluster-picker window modes - is registered and handled in
//! `util::shell::saved_layouts`, not here: this module owns only the list
//! view itself, so it can be built and tested on its own, the same split
//! `ui::picker::ClusterPicker` and this crate's command palette
//! (`util::palette`) already use.
//!
//! Split by concern, each an inherent `impl SavedLayoutsPicker` slice or a
//! group of free functions: [`state`] owns construction and the list/rename
//! state; [`interaction`] owns click/keyboard selection and the rename/delete
//! commands; [`render`] wires both into `Render for SavedLayoutsPicker`. This
//! file stays the module root: shared imports, submodule declarations, the
//! two registered commands this section owns, and the re-export the rest of
//! the crate reaches through `ui::picker::saved_layouts::`.
//!
//! Loading the selected layout (tasks section 4) acts through
//! [`SavedLayoutsPicker::selected`] and [`SavedLayoutsPicker::main_window`]:
//! `saved_layouts.load_replace` (`enter`) and `saved_layouts.load_add`
//! (`secondary-enter`) are registered below, with their `on_action`s in
//! [`interaction`] and the capture/apply logic itself in
//! `util::shell::saved_layouts::load` (design.md D3 - that decoder is
//! `pub(super)` inside `util::shell`, so the code that reads it lives there,
//! not here). `enter` is also gpui-component's own `Command` widget's built-in
//! confirm key, bound in its own `"Command"` key context, which sits deeper
//! in the render tree than this picker's own and so wins the keystroke before
//! a plain `on_action` for `LoadReplace` ever would - [`render`] wires
//! `Command::on_confirm` to the very method `on_action_load_replace` calls,
//! so Enter reaches Replace exactly once whichever of the two paths
//! dispatches it (see `interaction`'s own doc comment on
//! `confirm_selected`/`load_selected`).

use crate::command::{Command as RegisteredCommand, CommandRegistry};
use crate::config::saved_layouts::{self, RenameError, SavedLayout, UnreadableLayout};
use crate::util::shell::MainWindow;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{IndexPath, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;

mod interaction;
mod render;
mod state;

use state::RenameState;

pub use state::SavedLayoutsPicker;

actions!(
    saved_layouts_picker,
    [RenameSelected, DeleteSelected, LoadReplace, LoadAdd]
);

/// The key context this view's render root carries - active whenever the
/// picker has focus, so the palette offers `saved_layouts.rename_selected`/
/// `delete_selected` only then (design.md D4).
pub const KEY_CONTEXT: &str = "SavedLayoutsPicker";
/// Where `r`/`backspace` are actually bound: the picker minus its rename
/// field, so typing a new name never fires either command instead of typing
/// (`keyboard-first.md`) - the same `&& !Input` convention every other
/// panel's bare-key commands use.
const KEYS_CONTEXT: &str = "SavedLayoutsPicker && !Input";

pub(crate) const RENAME_SELECTED_COMMAND_ID: &str = "saved_layouts.rename_selected";
pub(crate) const RENAME_SELECTED_DEFAULT_BINDING: &str = "r";
pub(crate) const DELETE_SELECTED_COMMAND_ID: &str = "saved_layouts.delete_selected";
pub(crate) const DELETE_SELECTED_DEFAULT_BINDING: &str = "backspace";
/// [`crate::ui::saved_layout_delete::confirm_delete`]'s `id_prefix` for the
/// delete confirmation's buttons - the same helper the Settings Layouts
/// section's Remove control calls too (design.md D6), so deleting reads,
/// looks and keys alike from both surfaces.
const DELETE_ID_PREFIX: &str = "saved-layouts-delete";
pub(crate) const LOAD_REPLACE_COMMAND_ID: &str = "saved_layouts.load_replace";
pub(crate) const LOAD_REPLACE_DEFAULT_BINDING: &str = "enter";
pub(crate) const LOAD_ADD_COMMAND_ID: &str = "saved_layouts.load_add";
/// The app's existing cross-platform idiom for a modified-Enter variant
/// (`cmd-enter` on macOS, `ctrl-enter` elsewhere) - the Pods and Resource-list
/// panels' own open-in-background binding, per design.md D4.
pub(crate) const LOAD_ADD_DEFAULT_BINDING: &str = "secondary-enter";

/// `saved_layouts.rename_selected`, `saved_layouts.delete_selected`,
/// `saved_layouts.load_replace` and `saved_layouts.load_add` (design.md D4's
/// table) - none with a menu entry, since all four act on whichever row is
/// highlighted rather than naming a fixed target a menu item could read.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(RegisteredCommand {
        id: RENAME_SELECTED_COMMAND_ID,
        // design.md D4's table titles this one "Rename" (not "Rename Saved
        // Layout"): the picker is already the one place renaming happens, so
        // the palette entry doesn't need to repeat what it's renaming.
        title: "Rename",
        default_binding: RENAME_SELECTED_DEFAULT_BINDING,
        context: Some(KEYS_CONTEXT),
        action: Box::new(RenameSelected),
        menu: None,
    });
    registry.register(RegisteredCommand {
        id: DELETE_SELECTED_COMMAND_ID,
        // design.md D4's table: "Delete".
        title: "Delete",
        default_binding: DELETE_SELECTED_DEFAULT_BINDING,
        context: Some(KEYS_CONTEXT),
        action: Box::new(DeleteSelected),
        menu: None,
    });
    registry.register(RegisteredCommand {
        id: LOAD_REPLACE_COMMAND_ID,
        title: "Load (Replace)",
        default_binding: LOAD_REPLACE_DEFAULT_BINDING,
        context: Some(KEYS_CONTEXT),
        action: Box::new(LoadReplace),
        menu: None,
    });
    registry.register(RegisteredCommand {
        id: LOAD_ADD_COMMAND_ID,
        title: "Load (Add)",
        default_binding: LOAD_ADD_DEFAULT_BINDING,
        context: Some(KEYS_CONTEXT),
        action: Box::new(LoadAdd),
        menu: None,
    });
}
