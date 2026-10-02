//! Drawing the Keyboard Shortcuts section: the filter, the rows, the recording
//! and conflict prompts, and the key hint row.

use super::super::rows::Row;
use super::{
    CONTEXT, FilterShortcuts, Mode, RecordShortcut, RemoveShortcut, ResetShortcut, ShortcutsSection,
};
use crate::command::CommandRegistry;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl Render for ShortcutsSection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.visible_rows(cx);
        let current = self.current(&rows);
        let theme = cx.theme().clone();
        let list = div()
            .id("keyboard-shortcuts-list")
            .key_context(CONTEXT)
            .track_focus(&self.list_focus)
            .on_action(cx.listener(Self::on_action_record))
            .on_action(cx.listener(Self::on_action_reset))
            .on_action(cx.listener(Self::on_action_remove))
            .on_action(cx.listener(Self::on_action_filter))
            .on_action(cx.listener(Self::on_action_next))
            .on_action(cx.listener(Self::on_action_previous))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(rows.iter().enumerate().map(|(ix, row)| {
                self.render_row(row, Some(ix) == current, cx)
                    .into_any_element()
            }));
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(crate::ui::space::spacing(cx).control_gap)
            .p(crate::ui::space::spacing(cx).panel_inset)
            .child(div().text_lg().child("Keyboard Shortcuts"))
            .child(Input::new(&self.filter))
            .children(
                self.error
                    .clone()
                    .map(|error| div().text_sm().text_color(theme.danger).child(error)),
            )
            .child(list)
            .child(hint_row(window, cx))
    }
}

impl ShortcutsSection {
    fn render_row(&self, row: &Row, selected: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let id = row.id;
        let recording = matches!(&self.mode, Mode::Recording { id: r, .. } if *r == id);
        let confirming = match &self.mode {
            Mode::Confirming {
                id: c,
                keys,
                others,
                ..
            } if *c == id => Some((keys.clone(), others.clone())),
            _ => None,
        };
        let key_label: AnyElement = if recording {
            div()
                .text_color(theme.primary)
                .child("Press a shortcut, or Escape to cancel")
                .into_any_element()
        } else {
            match &row.keys {
                Some(keys) => keys_element(keys).into_any_element(),
                None => div()
                    .text_color(theme.muted_foreground)
                    .child("None")
                    .into_any_element(),
            }
        };
        let this = cx.weak_entity();
        div()
            .id(SharedString::from(format!("shortcut-{id}")))
            .debug_selector(move || format!("shortcut-row-{id}"))
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .rounded_md()
            .when(selected, |this| {
                this.bg(crate::ui::style::accent_subtle(cx))
            })
            .on_click({
                let this = this.clone();
                move |_event, _window, cx| {
                    let _ = this.update(cx, |this, cx| this.select(id, cx));
                }
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(row.title))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(row.scope.clone()),
                    )
                    .child(key_label)
                    .when(row.changed, |this| {
                        this.child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("changed"),
                        )
                    })
                    .when(selected && !recording && confirming.is_none(), |row_el| {
                        row_el.child(self.row_buttons(row, cx))
                    }),
            )
            .children(row.notes.iter().map(|note| {
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(note.clone())
            }))
            .children(confirming.map(|(keys, others)| {
                let registry = cx.global::<CommandRegistry>();
                let names: Vec<&str> = others
                    .iter()
                    .map(|other| registry.get(other).map_or(*other, |c| c.title))
                    .collect();
                let apply = cx.weak_entity();
                let cancel = cx.weak_entity();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .child(format!(
                        "{keys} is also used by {}. Apply anyway? (Enter / Escape)",
                        names.join(", ")
                    ))
                    .child(
                        Button::new(SharedString::from(format!("shortcut-apply-{id}")))
                            .label("Apply")
                            .xsmall()
                            .on_click(move |_event, _window, cx| {
                                let _ = apply.update(cx, |this, cx| this.confirm(cx));
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("shortcut-cancel-{id}")))
                            .label("Cancel")
                            .xsmall()
                            .ghost()
                            .on_click(move |_event, _window, cx| {
                                let _ = cancel.update(cx, |this, cx| this.stop(cx));
                            }),
                    )
            }))
    }

    /// Change / Reset / Remove for the selected row - the mouse route to the
    /// same actions its keys run.
    fn row_buttons(&self, row: &Row, cx: &mut Context<Self>) -> impl IntoElement {
        let id = row.id;
        let (change, reset, remove) = (cx.weak_entity(), cx.weak_entity(), cx.weak_entity());
        div()
            .flex()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!("shortcut-change-{id}")))
                    .label("Change…")
                    .xsmall()
                    .on_click(move |_event, window, cx| {
                        let _ = change.update(cx, |this, cx| {
                            this.select(id, cx);
                            this.record(window, cx);
                        });
                    }),
            )
            .when(row.changed, |this| {
                this.child(
                    Button::new(SharedString::from(format!("shortcut-reset-{id}")))
                        .label("Reset")
                        .xsmall()
                        .ghost()
                        .on_click(move |_event, _window, cx| {
                            let _ = reset.update(cx, |this, cx| this.reset(cx));
                        }),
                )
            })
            .when(row.keys.is_some(), |this| {
                this.child(
                    Button::new(SharedString::from(format!("shortcut-remove-{id}")))
                        .label("Remove")
                        .xsmall()
                        .ghost()
                        .on_click(move |_event, _window, cx| {
                            let _ = remove.update(cx, |this, cx| this.remove(cx));
                        }),
                )
            })
    }
}

/// A binding as key caps - one per keystroke, so chords show both.
fn keys_element(keys: &str) -> impl IntoElement {
    div().flex().gap_1().children(
        keys.split_whitespace()
            .map(|key| match Keystroke::parse(key) {
                Ok(keystroke) => Kbd::new(keystroke).into_any_element(),
                Err(_) => div().child(key.to_string()).into_any_element(),
            }),
    )
}

/// The section's keys, read from the live keymap like every hint row.
fn hint_row(window: &mut Window, cx: &mut Context<ShortcutsSection>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let hint = |action: &dyn Action, label: &'static str| {
        div()
            .flex()
            .items_center()
            .gap_1()
            .children(Kbd::binding_for_action(action, Some(CONTEXT), window))
            .child(label)
    };
    div()
        .flex()
        .flex_wrap()
        .gap(crate::ui::space::spacing(cx).control_gap)
        .px_2()
        .py_1()
        .rounded_md()
        .bg(crate::ui::style::surface_raised(cx))
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(hint(&RecordShortcut, "Change"))
        .child(hint(&ResetShortcut, "Reset"))
        .child(hint(&RemoveShortcut, "Remove"))
        .child(hint(&FilterShortcuts, "Filter"))
}
