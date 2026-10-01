//! What a window is called: the one string its native title bar, the macOS
//! Window menu and the accessibility tree all show (`window-title-and-menu`
//! design.md decisions 1 and 4).

use super::*;
use crate::consts::APP_NAME;

/// A window's title, from its mode: the app name alone in the cluster picker,
/// `"<context> - Fernrohr"` for one context, and `"<n> clusters - Fernrohr"`
/// for several. A count rather than a list of names, because a window can
/// hold any number of contexts and macOS truncates long Window-menu entries
/// (design.md decision 4).
pub(super) fn title_for(mode: &WindowMode) -> String {
    match mode {
        WindowMode::Picker(_) => title_from(&[], 0),
        WindowMode::Workspace {
            contexts, active, ..
        } => title_from(contexts, *active),
    }
}

/// The title a window opened onto `contexts` starts with, before its mode
/// exists: `open_window` needs it for `TitlebarOptions`, and the mode is built
/// inside the window. `enter_workspace` makes the first context active, so this
/// is what [`title_for`] reads once the window is up - a restored window is
/// titled from its first frame rather than blank (design.md decision 2).
pub(super) fn initial_title(contexts: &[String]) -> String {
    title_from(contexts, 0)
}

/// Whether `handle` shows the `MainWindow` entity `id`: wrapped in `Root`, as
/// `open_window` opens it, or as the root view itself, as tests open it.
fn hosts(handle: AnyWindowHandle, id: EntityId, cx: &App) -> bool {
    let wrapped = handle
        .downcast::<Root>()
        .and_then(|root| root.read(cx).ok())
        .is_some_and(|root| root.view().entity_id() == id);
    wrapped
        || handle
            .downcast::<MainWindow>()
            .and_then(|main_window| main_window.entity(cx).ok())
            .is_some_and(|main_window| main_window.entity_id() == id)
}

/// [`title_for`] over the parts of a mode it reads, so the strings are
/// testable without building a window. An `active` past the end of a
/// one-context list falls back to the app name, the same title a window with
/// nothing to name gets.
fn title_from(contexts: &[String], active: usize) -> String {
    match contexts {
        [] => APP_NAME.to_string(),
        [_] => contexts.get(active).map_or_else(
            || APP_NAME.to_string(),
            |name| format!("{name} - {APP_NAME}"),
        ),
        several => format!("{} clusters - {APP_NAME}", several.len()),
    }
}

/// Pushes `mode`'s title to `window`: the native title bar, the macOS Window
/// menu entry and the accessibility name, all from one `set_window_title`
/// (design.md decision 1). For callers holding the window, which always pass
/// the mode they just set - so the title cannot lag the mode.
pub(super) fn apply(mode: &WindowMode, window: &mut Window) {
    window.set_window_title(&title_for(mode));
}

/// [`apply`] for a caller with no `Window` - `sync_context_children`, from
/// inside its `cx.defer` (design.md decision 6). Finds the window whose root
/// view is `main_window` and reads the title off that window's mode as it is
/// now, not as it was when the defer was queued. A no-op if the window has
/// closed in between.
pub(super) fn apply_deferred(main_window: &WeakEntity<MainWindow>, cx: &mut App) {
    let Some(main_window) = main_window.upgrade() else {
        return;
    };
    let id = main_window.entity_id();
    let handle = cx
        .windows()
        .into_iter()
        .find(|handle| hosts(*handle, id, cx));
    let Some(handle) = handle else {
        return;
    };
    let title = title_for(&main_window.read(cx).mode);
    let _ = handle.update(cx, |_, window, _| window.set_window_title(&title));
}

#[cfg(test)]
mod tests;
