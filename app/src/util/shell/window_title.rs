//! What a window is called: the one string its native title bar, the macOS
//! Window menu and the accessibility tree all show (`window-title-and-menu`
//! design.md decisions 1 and 4).

use super::*;
use crate::consts::APP_NAME;

// UNWIRED(#60): `open_window` (section 2) and `sync_context_children` /
// `enter_picker` (section 3) are the callers; until they land nothing reads it.
#[allow(dead_code)]
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

#[cfg(test)]
mod tests;
