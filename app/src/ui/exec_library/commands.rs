//! The exec library's keyboard routes as registered commands (D5): opening the
//! window from anywhere; Run, New, Edit, Duplicate and Delete in the window's
//! list; Save and Run in the entry dialog. Each is in the palette while its
//! context is on the focus path, and its key is shown where it acts.
//!
//! The list's keys are single letters, so they're gated on `!Input`: typed in
//! the filter, `e` filters. Enter in the filter runs the highlighted entry
//! through the filter's own Enter, not this binding.

use crate::command::{Command, CommandRegistry, ContextGroup, MenuSlot};
use gpui_kit::{Action, actions};

actions!(
    exec_library,
    [
        OpenExecLibrary,
        NewEntry,
        RunEntry,
        EditEntry,
        DuplicateEntry,
        DeleteEntry,
        SaveAndRun
    ]
);

/// The window's key context.
pub(crate) const WINDOW_CONTEXT: &str = "ExecLibraryWindow";
/// The entry dialog's key context.
pub(crate) const ENTRY_CONTEXT: &str = "ExecLibraryEntry";

pub(crate) const RUN_KEY: &str = "enter";
pub(crate) const NEW_KEY: &str = "n";
pub(crate) const EDIT_KEY: &str = "e";
pub(crate) const DUPLICATE_KEY: &str = "d";
pub(crate) const DELETE_KEY: &str = "secondary-backspace";
pub(crate) const SAVE_AND_RUN_KEY: &str = "secondary-enter";

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: "exec_library.open",
        title: "Open Exec Library",
        default_binding: "",
        context: None,
        action: Box::new(OpenExecLibrary),
        menu: Some(MenuSlot::Context(ContextGroup::Contexts)),
    });
    let list: [(&'static str, &'static str, &'static str, Box<dyn Action>); 5] = [
        (
            "exec_library.run",
            "Exec Library: Run",
            RUN_KEY,
            Box::new(RunEntry),
        ),
        (
            "exec_library.new",
            "Exec Library: New Entry…",
            NEW_KEY,
            Box::new(NewEntry),
        ),
        (
            "exec_library.edit",
            "Exec Library: Edit Entry…",
            EDIT_KEY,
            Box::new(EditEntry),
        ),
        (
            "exec_library.duplicate",
            "Exec Library: Duplicate Entry",
            DUPLICATE_KEY,
            Box::new(DuplicateEntry),
        ),
        (
            "exec_library.delete",
            "Exec Library: Delete Entry…",
            DELETE_KEY,
            Box::new(DeleteEntry),
        ),
    ];
    for (id, title, default_binding, action) in list {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some("ExecLibraryWindow && !Input"),
            action,
            menu: None,
        });
    }
    registry.register(Command {
        id: "exec_library.save_and_run",
        title: "Exec Library: Save and Run",
        default_binding: SAVE_AND_RUN_KEY,
        context: Some(ENTRY_CONTEXT),
        action: Box::new(SaveAndRun),
        menu: None,
    });
}
