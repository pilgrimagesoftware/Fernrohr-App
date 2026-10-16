//! What the Exec Library window draws: a banner when the file is broken, the
//! toolbar's icon buttons (each with a tooltip naming it and its live key), the
//! filter, the entries, and a hint row of the list's keys.
//!
//! Up and Down move the highlight from the filter or the list - pure cursor
//! movement - and Enter in the filter runs the highlighted entry. Tab reaches
//! the list, where the single-letter keys act.

use super::LibraryWindow;
use crate::ui::exec_library::commands::*;
use crate::ui::exec_library::dialog::{self, Origin};
use crate::ui::exec_library::store::ExecLibraryStore;
use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Enter, Input};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Element ids and selectors, for tests.
pub(crate) const FILTER_ID: &str = "exec-library-filter";
pub(crate) const BANNER_SELECTOR: &str = "exec-library-broken";
pub(crate) fn row_selector(index: usize) -> String {
    format!("exec-library-row {index}")
}
pub(crate) fn toolbar_id(name: &str) -> SharedString {
    format!("exec-library-{name}").into()
}

impl LibraryWindow {
    pub(super) fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        dialog::open(None, Origin::Library(self.opener.clone()), window, cx);
    }

    pub(super) fn open_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selected_entry(cx) {
            dialog::open(
                Some(entry),
                Origin::Library(self.opener.clone()),
                window,
                cx,
            );
        }
    }

    fn toolbar(&self, read_only: bool) -> Div {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(tool("run", IconName::Play, "Run", Box::new(RunEntry)))
            .when(!read_only, |bar| {
                bar.child(tool("new", IconName::Plus, "New entry", Box::new(NewEntry)))
                    .child(tool(
                        "edit",
                        IconName::Pencil,
                        "Edit entry",
                        Box::new(EditEntry),
                    ))
                    .child(tool(
                        "duplicate",
                        IconName::Copy,
                        "Duplicate entry",
                        Box::new(DuplicateEntry),
                    ))
                    .child(tool(
                        "delete",
                        IconName::Trash,
                        "Delete entry",
                        Box::new(DeleteEntry),
                    ))
            })
    }

    fn hints(&self, read_only: bool, window: &mut Window) -> Div {
        use crate::ui::list_search::ListSearch;
        let hint = |action: &dyn Action, key: &str, label: &'static str, window: &mut Window| {
            ListSearch::hint(action, WINDOW_CONTEXT, key, label, window)
        };
        div()
            .flex()
            .flex_wrap()
            .gap_3()
            .text_sm()
            .child(hint(&RunEntry, RUN_KEY, "Run", window))
            .when(!read_only, |row| {
                row.child(hint(&NewEntry, NEW_KEY, "New", window))
                    .child(hint(&EditEntry, EDIT_KEY, "Edit", window))
                    .child(hint(&DuplicateEntry, DUPLICATE_KEY, "Duplicate", window))
                    .child(hint(&DeleteEntry, DELETE_KEY, "Delete", window))
            })
    }
}

/// A toolbar icon button dispatching `action`, under a tooltip naming it and
/// showing its live key - findable by `icon_tooltip::selector(label)`.
fn tool(name: &str, icon: IconName, label: &'static str, action: Box<dyn Action>) -> Stateful<Div> {
    use gpui_kit::component::kbd::Kbd;
    use gpui_kit::component::tooltip::Tooltip;
    let (dispatched, shown) = (action.boxed_clone(), action);
    div()
        .id(SharedString::from(format!("{}-tip", toolbar_id(name))))
        .tooltip(move |window, cx| {
            let shown = shown.boxed_clone();
            Tooltip::element(move |window, _| {
                div()
                    .debug_selector(move || crate::ui::icon_tooltip::selector(label))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(label)
                    .children(Kbd::binding_for_action(
                        &*shown,
                        Some(WINDOW_CONTEXT),
                        window,
                    ))
            })
            .build(window, cx)
        })
        .child(
            Button::new(toolbar_id(name))
                .icon(icon)
                .xsmall()
                .ghost()
                .on_click(move |_, window, cx| {
                    window.dispatch_action(dispatched.boxed_clone(), cx)
                }),
        )
}

impl Render for LibraryWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let space = crate::ui::space::spacing(cx);
        let broken = ExecLibraryStore::broken(cx).map(str::to_string);
        let read_only = broken.is_some();
        let entries = self.entries(cx);
        let selected = self.selected;
        let rows = entries.iter().enumerate().map(|(index, entry)| {
            let this = cx.weak_entity();
            div()
                .id(SharedString::from(row_selector(index)))
                .debug_selector(move || row_selector(index))
                .flex()
                .flex_col()
                .px(space.panel_inset)
                .py_1()
                .rounded_md()
                .when(index == selected, |row| row.bg(theme.selection))
                .on_click(move |event, window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.selected = index;
                        if event.click_count() > 1 {
                            this.run_selected(window, cx);
                        }
                        cx.notify();
                    });
                })
                .child(div().child(entry.name.clone()))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(format!("{}  ·  {}", entry.command, entry.target.describe())),
                )
        });
        let empty = entries.is_empty().then(|| {
            div()
                .p(space.panel_inset)
                .text_color(theme.muted_foreground)
                .child(if ExecLibraryStore::get(cx).entries.is_empty() {
                    "No entries yet. New Entry… makes one."
                } else {
                    "No entry matches the filter."
                })
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .p(space.panel_inset)
            .bg(theme.background)
            .key_context(WINDOW_CONTEXT)
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let delta = match event.keystroke.key.as_str() {
                    "down" if !event.keystroke.modifiers.modified() => 1,
                    "up" if !event.keystroke.modifiers.modified() => -1,
                    _ => return,
                };
                this.move_selection(delta, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &RunEntry, window, cx| this.run_selected(window, cx)))
            .when(!read_only, |root| {
                root.on_action(cx.listener(|this, _: &NewEntry, window, cx| this.open_new(window, cx)))
                    .on_action(cx.listener(|this, _: &EditEntry, window, cx| this.open_edit(window, cx)))
                    .on_action(cx.listener(|this, _: &DuplicateEntry, window, cx| {
                        this.duplicate_selected(window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &DeleteEntry, window, cx| {
                        this.delete_selected(window, cx)
                    }))
            })
            .children(broken.map(|reason| {
                div()
                    .debug_selector(|| BANNER_SELECTOR.into())
                    .p(space.control_gap)
                    .rounded_md()
                    .border_1()
                    .border_color(theme.danger)
                    .text_color(theme.danger)
                    .child(format!(
                        "The library can't be read, so it is read-only until the file is fixed. {reason}"
                    ))
            }))
            .child(self.toolbar(read_only))
            .child(
                div()
                    .on_action(cx.listener(|this, _: &Enter, window, cx| this.run_selected(window, cx)))
                    .child(Input::new(&self.filter).id(FILTER_ID)),
            )
            .child(
                div()
                    .id("exec-library-list")
                    .track_focus(&self.list_focus)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(rows)
                    .children(empty),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(crate::ui::style::surface_raised(cx))
                    .child(self.hints(read_only, window)),
            )
    }
}
