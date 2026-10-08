//! The Settings window's Layouts section (`saved-panel-layouts` design.md
//! D7): lists every saved layout by name (`config::saved_layouts::load_all`)
//! and lets the user remove one, through the same Irreversible confirmation
//! the saved layouts picker's own delete uses
//! (`crate::ui::saved_layout_delete::confirm_delete`, design.md D6), so
//! deleting reads, looks and keys alike from either surface. Renaming stays
//! in the picker (D4); this section only lists and removes.
//!
//! The list is read off the main thread, never while rendering
//! ([`LayoutsCache`]): `SettingsWindow` loads it in the background when the
//! section is shown, and again whenever a save, rename or removal anywhere
//! bumps `util::shell::SavedLayoutsChanged` while it is shown. Rendering reads
//! only the cache, saying it's loading until the first load lands.

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

/// The saved layouts as last read off the main thread, and which read is the
/// latest - so an older read that lands late never replaces a newer one.
#[derive(Default)]
pub(super) struct LayoutsCache {
    /// `None` until the first read lands.
    loaded: Option<(Vec<String>, Vec<UnreadableLayout>)>,
    generation: u64,
}

impl SettingsWindow {
    /// Reads the saved layouts off the main thread into the cache, keeping
    /// what it showed until the read lands, then redrawing.
    pub(super) fn reload_layouts(&mut self, cx: &mut Context<Self>) {
        self.layouts.generation += 1;
        let generation = self.layouts.generation;
        let dir = crate::util::shell::layouts_dir(cx);
        let read = cx.background_spawn(async move { saved_layouts::load_all(&dir) });
        cx.spawn(async move |this, cx| {
            let (layouts, unreadable) = read.await;
            let names = layouts.into_iter().map(|layout| layout.name).collect();
            let _ = this.update(cx, |this, cx| {
                if this.layouts.generation == generation {
                    this.layouts.loaded = Some((names, unreadable));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

/// The loading message's debug selector.
pub(super) const LOADING_SELECTOR: &str = "settings-layouts-loading";
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
pub(super) fn section(cache: &LayoutsCache, cx: &App) -> impl IntoElement {
    let space = crate::ui::space::spacing(cx);
    let Some((layouts, unreadable)) = &cache.loaded else {
        return div()
            .p(space.panel_inset)
            .child(
                div()
                    .debug_selector(|| LOADING_SELECTOR.into())
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading saved layouts\u{2026}"),
            )
            .into_any_element();
    };
    let dir = crate::util::shell::layouts_dir(cx);

    let empty = layouts.is_empty().then(|| {
        div()
            .debug_selector(|| EMPTY_SELECTOR.into())
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("No saved layouts yet.")
    });

    let rows = layouts
        .iter()
        .enumerate()
        .map(|(index, name)| row(index, name.clone(), dir.clone()));

    let unreadable_notice = (!unreadable.is_empty()).then(|| unreadable_list(unreadable, cx));

    div()
        .p(space.panel_inset)
        .flex()
        .flex_col()
        .gap(space.control_gap)
        .children(empty)
        .children(rows)
        .children(unreadable_notice)
        .into_any_element()
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
fn row(index: usize, name: String, dir: PathBuf) -> impl IntoElement {
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
                    // The removal bumps `SavedLayoutsChanged`, which re-reads
                    // the list; nothing more to do here.
                    saved_layout_delete::confirm_delete(
                        &name_for_click,
                        DELETE_ID_PREFIX,
                        dir.clone(),
                        |_window, _cx| {},
                        window,
                        cx,
                    );
                }),
        ))
}
