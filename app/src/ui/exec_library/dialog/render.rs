//! What the entry dialog draws: each field with its label and, under it, its
//! error; the target kind and the workload kind as rows of buttons - each a
//! tab stop, the chosen one filled; and the footer's Cancel, Save and Run, and
//! Save, each showing its key.
//!
//! Enter in a field saves, `cmd-enter` saves and runs - both through the
//! field's own Enter, which the dialog would otherwise take as plain OK.

use super::{EntryDialog, TargetKind};
use crate::config::exec_library::{Field, WorkloadKind};
use crate::ui::confirm_dialog;
use crate::ui::exec_library::commands::{ENTRY_CONTEXT, SAVE_AND_RUN_KEY, SaveAndRun};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Enter, Input, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::*;

/// Ids and selectors, for tests.
pub(crate) const NAME_ID: &str = "exec-entry-name";
pub(crate) const COMMAND_ID: &str = "exec-entry-command";
pub(crate) const SAVE_ID: &str = "exec-entry-save";
pub(crate) const SAVE_AND_RUN_ID: &str = "exec-entry-save-and-run";
pub(crate) const CANCEL_ID: &str = "exec-entry-cancel";
pub(crate) fn error_selector(field: Field) -> String {
    format!("exec-entry-error {field:?}")
}
pub(crate) fn target_kind_id(kind: TargetKind) -> SharedString {
    format!("exec-entry-target-{kind:?}").into()
}
pub(crate) fn workload_kind_id(kind: WorkloadKind) -> SharedString {
    format!("exec-entry-workload-{}", kind.as_str()).into()
}

/// Save and Run's key, from the live keymap.
fn save_and_run_key(window: &Window) -> Kbd {
    Kbd::binding_for_action(&SaveAndRun, Some(ENTRY_CONTEXT), window).unwrap_or_else(|| {
        Kbd::new(Keystroke::parse(SAVE_AND_RUN_KEY).expect("a valid default key"))
    })
}

/// Cancel, Save and Run, and Save, each showing its key.
pub(super) fn footer(dialog: &Entity<EntryDialog>, window: &mut Window, _cx: &App) -> DialogFooter {
    let (save, run) = (dialog.clone(), dialog.clone());
    DialogFooter::new()
        .child(div().flex_1())
        .child(
            Button::new(CANCEL_ID)
                .label("Cancel")
                .child(confirm_dialog::cancel_key(window))
                .on_click(|_, window, cx| window.close_dialog(cx)),
        )
        .child(
            Button::new(SAVE_AND_RUN_ID)
                .label("Save and Run")
                .child(save_and_run_key(window))
                .on_click(move |_, window, cx| {
                    run.update(cx, |dialog, cx| dialog.save(true, window, cx));
                }),
        )
        .child(
            Button::new(SAVE_ID)
                .label("Save")
                .primary()
                .child(confirm_dialog::confirm_key(window))
                .on_click(move |_, window, cx| {
                    save.update(cx, |dialog, cx| dialog.save(false, window, cx));
                }),
        )
}

impl EntryDialog {
    /// `input` under `label`, its error under it, Enter and `cmd-enter`
    /// saving from it.
    fn field(
        &self,
        label: &'static str,
        id: &'static str,
        input: &Entity<InputState>,
        field: Option<Field>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme();
        let error = field.and_then(|field| self.error_for(field).map(|error| (field, error)));
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .on_action(cx.listener(|this, enter: &Enter, window, cx| {
                        this.save(enter.secondary, window, cx)
                    }))
                    .child(Input::new(input).id(id)),
            )
            .children(error.map(|(field, error)| {
                div()
                    .debug_selector(move || error_selector(field))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(error)
            }))
    }

    fn choice_row<T: Copy + PartialEq + 'static>(
        &self,
        label: &'static str,
        choices: Vec<(T, SharedString, &'static str)>,
        chosen: T,
        on_choose: fn(&mut Self, T, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme();
        let buttons = choices.into_iter().map(|(value, id, text)| {
            let button = Button::new(id).label(text).small();
            let button = if value == chosen {
                button.primary()
            } else {
                button.outline()
            };
            button.on_click(cx.listener(move |this, _, _, cx| on_choose(this, value, cx)))
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(label),
            )
            .child(div().flex().flex_wrap().gap_1().children(buttons))
    }
}

impl Render for EntryDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.choice_row(
            "Target",
            vec![
                (TargetKind::Any, target_kind_id(TargetKind::Any), "Any pod"),
                (
                    TargetKind::Selector,
                    target_kind_id(TargetKind::Selector),
                    "Pod selector",
                ),
                (
                    TargetKind::Workload,
                    target_kind_id(TargetKind::Workload),
                    "Workload",
                ),
            ],
            self.target_kind,
            Self::set_target_kind,
            cx,
        );
        let mut target_fields: Vec<AnyElement> = Vec::new();
        match self.target_kind {
            TargetKind::Any => {}
            TargetKind::Selector => {
                let (namespace, selector) = (self.namespace.clone(), self.selector.clone());
                target_fields.push(
                    self.field(
                        "Namespace (empty for all)",
                        "exec-entry-namespace",
                        &namespace,
                        Some(Field::Namespace),
                        cx,
                    )
                    .into_any_element(),
                );
                target_fields.push(
                    self.field(
                        "Label selector",
                        "exec-entry-selector",
                        &selector,
                        Some(Field::Selector),
                        cx,
                    )
                    .into_any_element(),
                );
            }
            TargetKind::Workload => {
                let kinds = WorkloadKind::ALL
                    .into_iter()
                    .map(|kind| (kind, workload_kind_id(kind), kind.as_str()))
                    .collect();
                target_fields.push(
                    self.choice_row(
                        "Kind",
                        kinds,
                        self.workload_kind,
                        Self::set_workload_kind,
                        cx,
                    )
                    .into_any_element(),
                );
                let (namespace, name) = (self.namespace.clone(), self.workload_name.clone());
                target_fields.push(
                    self.field(
                        "Namespace",
                        "exec-entry-namespace",
                        &namespace,
                        Some(Field::Namespace),
                        cx,
                    )
                    .into_any_element(),
                );
                target_fields.push(
                    self.field(
                        "Name",
                        "exec-entry-workload-name",
                        &name,
                        Some(Field::WorkloadName),
                        cx,
                    )
                    .into_any_element(),
                );
            }
        }
        let (name, command, container) = (
            self.name.clone(),
            self.command.clone(),
            self.container.clone(),
        );
        let danger = cx.theme().danger;
        div()
            .key_context(ENTRY_CONTEXT)
            .on_action(cx.listener(|this, _: &SaveAndRun, window, cx| this.save(true, window, cx)))
            .flex()
            .flex_col()
            .gap_3()
            .child(self.field("Name", NAME_ID, &name, Some(Field::Name), cx))
            .child(self.field("Command", COMMAND_ID, &command, Some(Field::Command), cx))
            .child(target)
            .children(target_fields)
            .child(self.field(
                "Container (empty for the default)",
                "exec-entry-container",
                &container,
                None,
                cx,
            ))
            .children(
                self.general_error
                    .clone()
                    .map(|error| div().text_sm().text_color(danger).child(error)),
            )
    }
}
