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
//! Loading the selected layout (`saved_layouts.load_replace`/`load_add`,
//! tasks section 4) is a later change, not this one: [`SavedLayoutsPicker::
//! selected`] and [`SavedLayoutsPicker::main_window`] already expose what
//! that needs - the highlighted layout and a handle back to the window it
//! should act on - so section 4 only has to register the two commands and
//! add their `on_action`s here, not any new plumbing.

use crate::command::{Command as RegisteredCommand, CommandRegistry};
use crate::config::saved_layouts::{self, RenameError, SavedLayout, UnreadableLayout};
use crate::ui::confirm_dialog::{self, Confirmation, Severity};
use crate::ui::confirm_text::ConfirmText;
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

actions!(saved_layouts_picker, [RenameSelected, DeleteSelected]);

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
/// [`confirm_dialog::open`]'s `id_prefix` for the delete confirmation's
/// buttons.
const DELETE_ID_PREFIX: &str = "saved-layouts-delete";

/// `saved_layouts.rename_selected` and `saved_layouts.delete_selected`
/// (design.md D4's table). `load_replace`/`load_add` are section 4's own
/// registration, once it adds the commands those `on_action`s need.
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
}
