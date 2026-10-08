//! The Settings window's Layouts section (`saved-panel-layouts` design.md
//! D7): lists every saved layout by name (`config::saved_layouts::load_all`)
//! and lets the user remove one, through the same Irreversible confirmation
//! the saved layouts picker's own delete uses
//! (`crate::ui::saved_layout_delete::confirm_delete`, design.md D6), so
//! deleting reads, looks and keys alike from either surface. Renaming stays
//! in the picker (D4); this section only lists and removes.
//!
//! No state of its own: [`section`] reads [`saved_layouts::load_all`] fresh
//! on every call, the same stateless placement [`super::panels::section`]
//! uses for its own live-config row. A removal from this section's own
//! Remove control, or from the picker in a different window, shows up the
//! next time this section renders - which `SettingsWindow::show` always
//! does when the user switches to it (it calls `cx.notify()`
//! unconditionally), so "the next show" always re-reads the directory.
//! Removing from *this* section's own Remove control needs one more push:
//! the confirmation's `on_confirm` runs after the click that opened it has
//! already been dispatched, so nothing would otherwise ask `SettingsWindow`
//! to render again - [`section`] is handed a `WeakEntity<SettingsWindow>`
//! for exactly that, calling `cx.notify()` on it once the file is gone.

use super::SettingsWindow;
use crate::config::saved_layouts::{self, UnreadableLayout};
use crate::ui::icon_tooltip;
use crate::ui::saved_layout_delete;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::*;
use std::path::PathBuf;

/// The empty-list message's debug selector.
pub(super) const EMPTY_SELECTOR: &str = "settings-layouts-empty";
/// One unreadable file's notice.
pub(super) const UNREADABLE_SELECTOR: &str = "settings-layouts-unreadable";
/// The Remove control's tooltip text (`icon-buttons.md`: names the action in
/// plain words).
pub(super) const REMOVE_TOOLTIP: &str = "Remove saved layout";
/// [`saved_layout_delete::confirm_delete`]'s `id_prefix` for this section's
/// own confirmation buttons - distinct from the picker's own
/// (`ui::picker::saved_layouts`'s `DELETE_ID_PREFIX`), so the two surfaces'
/// dialogs never share a button id even though they read, look and key
/// alike.
const DELETE_ID_PREFIX: &str = "settings-layouts-delete";

/// Row `index`'s Remove button's element id.
pub(super) fn remove_button_id(index: usize) -> String {
    format!("settings-layouts-remove-{index}")
}

/// The debug selector of row `index`'s name text, for tests.
pub(super) fn row_name_selector(index: usize) -> String {
    format!("settings-layouts-row-name-{index}")
}

/// The section: every saved layout's name and Remove control, the explicit
/// "none yet" message in their place when there are none, and any unreadable
/// file named alongside the layouts that did parse - the same wording the
/// saved layouts picker uses for both (`ui::picker::saved_layouts::render`).
pub(super) fn section(settings_window: WeakEntity<SettingsWindow>, cx: &App) -> impl IntoElement {
    let space = crate::ui::space::spacing(cx);
    let dir = crate::util::shell::layouts_dir(cx);
    let (layouts, unreadable) = saved_layouts::load_all(&dir);

    let empty = layouts.is_empty().then(|| {
        div()
            .debug_selector(|| EMPTY_SELECTOR.into())
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("No saved layouts yet.")
    });

    let rows = layouts
        .into_iter()
        .enumerate()
        .map(|(index, layout)| row(index, layout.name, dir.clone(), settings_window.clone()));

    let unreadable_notice = (!unreadable.is_empty()).then(|| unreadable_list(&unreadable, cx));

    div()
        .p(space.panel_inset)
        .flex()
        .flex_col()
        .gap(space.control_gap)
        .children(empty)
        .children(rows)
        .children(unreadable_notice)
}

/// Every unreadable file, named - the same notice text
/// `ui::picker::saved_layouts::render`'s `UNREADABLE_SELECTOR` rows use.
fn unreadable_list(files: &[UnreadableLayout], cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .text_sm()
        .text_color(cx.theme().danger)
        .children(files.iter().map(|file| {
            div()
                .debug_selector(|| UNREADABLE_SELECTOR.into())
                .child(format!(
                    "{} could not be read as a saved layout.",
                    file.filename
                ))
        }))
}

/// One row: the layout's name, and its Remove control - a tab stop reachable
/// by Tab, activated by Enter or Space (`icon-buttons.md`, `keyboard-first.md`).
fn row(
    index: usize,
    name: String,
    dir: PathBuf,
    settings_window: WeakEntity<SettingsWindow>,
) -> impl IntoElement {
    let name_for_click = name.clone();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(
            div()
                .debug_selector(move || row_name_selector(index))
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(name),
        )
        .child(icon_tooltip::with_tooltip(
            SharedString::from(format!("{}-tooltip", remove_button_id(index))),
            REMOVE_TOOLTIP,
            Button::new(remove_button_id(index))
                .icon(IconName::Trash)
                .ghost()
                .xsmall()
                .on_click(move |_event, window, cx| {
                    let settings_window = settings_window.clone();
                    saved_layout_delete::confirm_delete(
                        &name_for_click,
                        DELETE_ID_PREFIX,
                        dir.clone(),
                        move |_window, cx| {
                            let _ = settings_window.update(cx, |_, cx| cx.notify());
                        },
                        window,
                        cx,
                    );
                }),
        ))
}
