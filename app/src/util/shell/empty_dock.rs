//! What the panel area shows once a connected window's last panel has closed:
//! the key that opens the next kind, from the live keymap. The window stays
//! connected (`tab-close-buttons` 4.1), so this replaces the picker that used
//! to appear here.

use super::*;
use crate::ui::resource_panel::FocusResources;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::kbd::Kbd;

/// The empty panel area's hint: "Press ⌘0 to open a kind from the Resource
/// panel", with ⌘0 read from the keymap. Without a binding it names the
/// command, so the hint never points at a key that does nothing.
pub(super) fn empty_dock_hint(window: &mut Window, cx: &App) -> AnyElement {
    let key = Kbd::binding_for_action(&FocusResources, None, window);
    let line = div()
        .flex()
        .items_center()
        .gap_1()
        .text_color(cx.theme().muted_foreground)
        .debug_selector(|| "empty-dock-hint".into());
    let line = match key {
        Some(key) => line
            .child("Press")
            .child(key)
            .child("to open a kind from the Resource panel"),
        None => line.child("Use Focus Resources to open a kind from the Resource panel"),
    };
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(line)
        .into_any_element()
}
