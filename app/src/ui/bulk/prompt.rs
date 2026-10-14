//! A small dialog asking for one or two values before a bulk action confirms:
//! Scale's replica count, Label's and Annotate's key and value. Enter in a
//! field (or Continue) submits, Escape (or Cancel) closes; a value the action
//! can't take shows why under the fields and stays open.
//!
//! Owns asking and checking; what the values mean is the caller's `parse`.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Enter, Input, InputEvent, InputState};
use gpui_kit::*;
use std::rc::Rc;

/// One field: its label and placeholder.
pub(crate) struct Field {
    pub(crate) label: &'static str,
    pub(crate) placeholder: &'static str,
}

/// What a prompt asks.
pub(crate) struct Prompt {
    pub(crate) title: SharedString,
    pub(crate) fields: Vec<Field>,
    /// The ids are `<id_prefix>-field-<n>`, `<id_prefix>-cancel`,
    /// `<id_prefix>-submit`, and the error line's selector `<id_prefix>-error`.
    pub(crate) id_prefix: &'static str,
}

/// The fields' values parsed, or why they can't be.
type Parse<T> = Rc<dyn Fn(&[String]) -> Result<T, String>>;
/// What a parsed answer goes on to.
type Submit<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

/// Opens `prompt`, focused on its first field. Each submit runs `parse` over
/// the fields' trimmed values; `Ok` closes the dialog and hands the value to
/// `on_submit`, `Err` shows its message.
pub(crate) fn open<T: 'static>(
    prompt: Prompt,
    parse: impl Fn(&[String]) -> Result<T, String> + 'static,
    on_submit: impl Fn(T, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let Prompt {
        title,
        fields,
        id_prefix,
    } = prompt;
    let view = cx.new(|cx| {
        PromptView::new(
            fields,
            id_prefix,
            Rc::new(parse),
            Rc::new(on_submit),
            window,
            cx,
        )
    });
    let first = view.read(cx).inputs.first().cloned();
    window.open_dialog(cx, move |dialog, _window, _cx| {
        let view = view.clone();
        dialog
            .w(px(420.))
            .title(title.clone())
            .content(move |content, _window, _cx| content.child(view.clone()))
    });
    if let Some(first) = first {
        let focus = first.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }
}

struct PromptView<T> {
    labels: Vec<&'static str>,
    inputs: Vec<Entity<InputState>>,
    id_prefix: &'static str,
    error: Option<SharedString>,
    parse: Parse<T>,
    on_submit: Submit<T>,
    _subscriptions: Vec<Subscription>,
}

impl<T: 'static> PromptView<T> {
    fn new(
        fields: Vec<Field>,
        id_prefix: &'static str,
        parse: Parse<T>,
        on_submit: Submit<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let inputs: Vec<Entity<InputState>> = fields
            .iter()
            .map(|field| cx.new(|cx| InputState::new(window, cx).placeholder(field.placeholder)))
            .collect();
        let subscriptions = inputs
            .iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this: &mut Self, _, event, _, cx| {
                    if let InputEvent::Change = event {
                        this.error = None;
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            labels: fields.iter().map(|field| field.label).collect(),
            inputs,
            id_prefix,
            error: None,
            parse,
            on_submit,
            _subscriptions: subscriptions,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let values: Vec<String> = self
            .inputs
            .iter()
            .map(|input| input.read(cx).value().trim().to_string())
            .collect();
        match (self.parse)(&values) {
            Ok(value) => {
                window.close_dialog(cx);
                (self.on_submit)(value, window, cx);
            }
            Err(error) => {
                self.error = Some(error.into());
                cx.notify();
            }
        }
    }
}

impl<T: 'static> Render for PromptView<T> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefix = self.id_prefix;
        let fields =
            self.labels
                .iter()
                .zip(&self.inputs)
                .enumerate()
                .map(|(ix, (label, input))| {
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().child(*label))
                        // Enter in a field submits; the input lets Enter bubble,
                        // and the dialog would otherwise close on it unsubmitted.
                        .child(
                            div()
                                .on_action(cx.listener(|this, _: &Enter, window, cx| {
                                    this.submit(window, cx)
                                }))
                                .child(
                                    Input::new(input)
                                        .id(SharedString::from(format!("{prefix}-field-{ix}"))),
                                ),
                        )
                });
        let danger = cx.theme().danger;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(fields)
            .children(self.error.clone().map(|error| {
                div()
                    .debug_selector(move || format!("{prefix}-error"))
                    .text_sm()
                    .text_color(danger)
                    .child(error)
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new(SharedString::from(format!("{prefix}-cancel")))
                            .label("Cancel")
                            .child(crate::ui::confirm_dialog::cancel_key(window))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new(SharedString::from(format!("{prefix}-submit")))
                            .label("Continue")
                            .primary()
                            .child(crate::ui::confirm_dialog::confirm_key(window))
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}
