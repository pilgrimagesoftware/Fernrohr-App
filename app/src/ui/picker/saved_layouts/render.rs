//! `Render for SavedLayoutsPicker`: the list (via `Command`, non-searchable
//! so a bare `r`/`backspace` never lands in a search field instead of firing
//! its command), the unreadable-file notice, and the keyboard hint row.
//! Wires `Command`'s `on_select` into [`super::interaction::
//! SavedLayoutsPicker::follow_keyboard`] the same way `ClusterPicker` does,
//! so hover never moves the keyboard selection (`keyboard-first.md`); wires
//! `Command`'s own built-in confirm (`enter`) into [`super::interaction::
//! SavedLayoutsPicker::confirm_selected`] the same way `ClusterPicker` wires
//! it to its own `confirm_row`, so a bare `enter` does Replace.

use super::*;

/// The empty-list message's debug selector.
pub(crate) const EMPTY_SELECTOR: &str = "saved-layouts-empty";
/// One unreadable file's notice.
pub(crate) const UNREADABLE_SELECTOR: &str = "saved-layouts-unreadable";
/// The rename field's element id.
pub(crate) const RENAME_INPUT_ID: &str = "saved-layouts-rename-input";
/// The rename row's inline collision-error line.
pub(crate) const RENAME_ERROR_SELECTOR: &str = "saved-layouts-rename-error";

impl Render for SavedLayoutsPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let selected = self.selected_index;
        let rename = self
            .rename
            .as_ref()
            .map(|rename| (rename.index, rename.input.clone(), rename.error.clone()));

        let items: Vec<CommandItem> = self
            .layouts
            .iter()
            .enumerate()
            .map(|(row_index, layout)| {
                let renaming = rename
                    .as_ref()
                    .filter(|(index, ..)| *index == row_index)
                    .map(|(_, input, error)| (input.clone(), error.clone()));
                CommandItem::new().label(layout.name.clone()).child(row(
                    layout.name.clone(),
                    row_index,
                    selected == Some(row_index),
                    renaming,
                    this.clone(),
                ))
            })
            .collect();

        let command = Command::new(&self.command_state)
            // No search field: the list itself is the only focusable
            // control, so a bare `r`/`backspace` reaches this view's own
            // commands rather than typing into a query box.
            .searchable(false)
            .items(items)
            .empty(|_state, _window, cx| {
                div()
                    .debug_selector(|| EMPTY_SELECTOR.into())
                    .py_6()
                    .w_full()
                    .text_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No saved layouts yet.")
            })
            .on_select({
                let this = this.clone();
                move |index_path, window, cx| {
                    if !window.last_input_was_keyboard() {
                        return;
                    }
                    let _ = this.update(cx, |this, cx| this.follow_keyboard(index_path.row, cx));
                }
            })
            // `Command`'s own built-in confirm action (bound to a bare
            // `enter` in its own `"Command"` key context, which `enter`
            // reaches before this view's own `LoadReplace` binding ever
            // would - see `interaction::SavedLayoutsPicker::confirm_selected`'s
            // doc comment). Routed to the same method `on_action_load_replace`
            // calls, so Enter does Replace exactly once either way.
            .on_confirm({
                let this = this.clone();
                move |_index_path, window, cx| {
                    let _ = this.update(cx, |this, cx| this.confirm_selected(window, cx));
                }
            });

        let theme = cx.theme().clone();
        let unreadable = (!self.unreadable.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .text_sm()
                .text_color(theme.danger)
                .children(self.unreadable.iter().map(|file| {
                    div()
                        .debug_selector(|| UNREADABLE_SELECTOR.into())
                        .child(format!(
                            "{} could not be read as a saved layout.",
                            file.filename
                        ))
                }))
        });

        div()
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(Self::on_action_rename_selected))
            .on_action(cx.listener(Self::on_action_delete_selected))
            .on_action(cx.listener(Self::on_action_load_replace))
            .on_action(cx.listener(Self::on_action_load_add))
            .w(px(420.))
            .flex()
            .flex_col()
            .gap_3()
            .child(command)
            .children(unreadable)
            .child(hint_row(window))
            .into_any_element()
    }
}

/// One row: the layout's name (or, while it is being renamed, the inline
/// field and any collision error), and the pencil button that starts a
/// rename - the mouse route alongside `r` (`keyboard-first.md`).
fn row(
    name: String,
    row_index: usize,
    selected: bool,
    renaming: Option<(Entity<InputState>, Option<SharedString>)>,
    picker: WeakEntity<SavedLayoutsPicker>,
) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_window, cx| {
        let theme = cx.theme();
        let picker_for_click = picker.clone();
        let picker_for_pencil = picker.clone();
        let name_for_pencil = name.clone();

        let row = div()
            .id(SharedString::from(format!("saved-layouts-row-{row_index}")))
            // A debug-only selector (separate from the `.id` above, which is
            // GPUI's own element identity): lets a test locate this row's
            // drawn bounds to simulate a real mouse hover over it, rather
            // than poking `CommandState` directly - the only way to exercise
            // the same `on_hover` -> `Command::select` -> our `on_select`
            // guard path a genuine hover goes through.
            .debug_selector(move || format!("saved-layouts-row-{row_index}"))
            .flex()
            .flex_1()
            .items_center()
            .justify_between()
            .gap_2()
            .px_1()
            .rounded(theme.radius)
            .when(selected, |row| row.bg(theme.selection))
            .on_click(move |_event, window, cx| {
                cx.stop_propagation();
                let _ = picker_for_click
                    .update(cx, |this, cx| this.handle_row_click(row_index, window, cx));
            });

        match &renaming {
            Some((input, error)) => row
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .flex_col()
                        .gap_1()
                        .on_action({
                            let picker = picker.clone();
                            move |_: &Enter, _window, cx| {
                                let _ = picker.update(cx, |this, cx| this.commit_rename(cx));
                            }
                        })
                        .on_action({
                            let picker = picker.clone();
                            move |_: &Escape, _window, cx| {
                                let _ = picker.update(cx, |this, cx| this.cancel_rename(cx));
                            }
                        })
                        .child(Input::new(input).id(RENAME_INPUT_ID))
                        .children(error.clone().map(|error| {
                            div()
                                .debug_selector(|| RENAME_ERROR_SELECTOR.into())
                                .text_sm()
                                .text_color(theme.danger)
                                .child(error)
                        })),
                )
                .into_any_element(),
            None => row
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(name.clone()),
                )
                .child(
                    Button::new(SharedString::from(format!(
                        "saved-layouts-rename-{row_index}"
                    )))
                    .icon(IconName::Pencil)
                    .xsmall()
                    .ghost()
                    .tooltip("Rename")
                    .on_click(move |_event, window, cx| {
                        let _ = picker_for_pencil.update(cx, |this, cx| {
                            this.start_rename(row_index, name_for_pencil.clone(), window, cx)
                        });
                    }),
                )
                .into_any_element(),
        }
    }
}

/// "↵ Load (Replace)  ⌘↵ Load (Add)  R Rename  ⌫ Delete", each key read from
/// the live keymap.
///
/// Looks up [`KEY_CONTEXT`] alone, not [`KEYS_CONTEXT`]: `Kbd::binding_for_action`'s
/// `context` parameter is `gpui`'s own `KeyContext` mini-language (a plain
/// active-context stack - one or more bare identifiers), not this crate's
/// `Command::is_available` predicate syntax (`"X && !Y"`) that `KEYS_CONTEXT`
/// is written in for `register_commands` below. `KeyContext::parse` has no
/// `&&`/`!` support at all - passing it a predicate string sends its
/// recursive-descent parser into a non-terminating loop on the first
/// unrecognized character, which overflows the stack rather than returning
/// an error (gpui-pre#0.3.7's `parse_expr`, which keeps recursing on an
/// unchanged remainder once it reads an empty identifier). The hint row is
/// drawn whenever the picker itself is on screen, never while its rename
/// field has focus and `!Input` would matter, so the plain context is exactly
/// right here, not just a workaround.
fn hint_row(window: &mut Window) -> impl IntoElement {
    let replace_key = Kbd::binding_for_action(&LoadReplace, Some(KEY_CONTEXT), window);
    let add_key = Kbd::binding_for_action(&LoadAdd, Some(KEY_CONTEXT), window);
    let rename_key = Kbd::binding_for_action(&RenameSelected, Some(KEY_CONTEXT), window);
    let delete_key = Kbd::binding_for_action(&DeleteSelected, Some(KEY_CONTEXT), window);
    div()
        .flex()
        .flex_wrap()
        .gap_3()
        .text_sm()
        .children(replace_key.map(|key| hint(key, "Load (Replace)")))
        .children(add_key.map(|key| hint(key, "Load (Add)")))
        .children(rename_key.map(|key| hint(key, "Rename")))
        .children(delete_key.map(|key| hint(key, "Delete")))
}

fn hint(key: Kbd, label: &'static str) -> impl IntoElement {
    div().flex().items_center().gap_1().child(key).child(label)
}

#[cfg(test)]
mod tests;
