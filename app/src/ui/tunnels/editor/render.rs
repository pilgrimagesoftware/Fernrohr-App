//! Owns the editor pane's layout: the field list with inline errors, the auth choice
//! and its private-key field, the connectivity-test status, Save/Cancel/Delete, and
//! the delete confirmation.

use super::*;
use crate::ui::confirm_text::ConfirmText;

impl Render for TunnelEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_new = self.editing_id.is_none();
        let is_ssh = self.kind == TunnelKind::Ssh;

        let field = |label: &'static str,
                     input: &Entity<InputState>,
                     error: Option<&'static str>| {
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
                .child(Input::new(input))
                .children(
                    error.map(|message| div().text_xs().text_color(theme.danger).child(message)),
                )
        };

        let host_error = self
            .field_errors
            .contains(&TunnelFieldError::EmptyHost)
            .then_some("Host is required.");
        let user_error = self
            .field_errors
            .contains(&TunnelFieldError::EmptyUser)
            .then_some("User is required.");
        let port_error = self
            .field_errors
            .contains(&TunnelFieldError::InvalidPort)
            .then_some("Port must be between 1 and 65535.");

        let auth_label = match self.auth {
            TunnelAuth::SshConfig => "SSH config/agent",
            TunnelAuth::KeychainKey => "Private key (keychain)",
        };
        let ssh_weak = cx.weak_entity();
        let key_weak = cx.weak_entity();
        let auth_button = Button::new("tunnel-auth")
            .label(auth_label)
            .icon(IconName::ChevronDown)
            .small()
            .outline()
            .dropdown_menu(move |menu, _window, _cx| {
                let ssh_weak = ssh_weak.clone();
                let key_weak = key_weak.clone();
                menu.item(PopupMenuItem::new("SSH config/agent").on_click(
                    move |_event, _window, cx| {
                        let _ = ssh_weak
                            .update(cx, |editor, cx| editor.set_auth(TunnelAuth::SshConfig, cx));
                    },
                ))
                .item(PopupMenuItem::new("Private key (keychain)").on_click(
                    move |_event, _window, cx| {
                        let _ = key_weak.update(cx, |editor, cx| {
                            editor.set_auth(TunnelAuth::KeychainKey, cx)
                        });
                    },
                ))
            });

        let key_field = matches!(self.auth, TunnelAuth::KeychainKey).then(|| {
            field(
                "Private key (leave blank to keep the existing one)",
                &self.key_material,
                None,
            )
        });

        let test_status = if self.testing {
            Some(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Testing…")
                    .into_any_element(),
            )
        } else {
            self.test_result.as_ref().map(|result| match result {
                Ok(()) => div()
                    .text_sm()
                    .text_color(theme.success)
                    .child("Reachable.")
                    .into_any_element(),
                Err(reason) => div()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(reason.clone())
                    .into_any_element(),
            })
        };

        let delete_confirm = self.confirming_delete.then(|| {
            let weak_confirm = cx.weak_entity();
            let weak_cancel = weak_confirm.clone();
            let message = delete_confirm_text(&self.bound_contexts);
            div()
                .flex()
                .flex_col()
                .gap(crate::ui::space::spacing(cx).control_gap)
                .p(crate::ui::space::spacing(cx).card_padding)
                .border_1()
                .border_color(theme.danger)
                .rounded_md()
                .child(div().text_sm().child(message.render(cx)))
                .child(
                    div()
                        .flex()
                        .gap(crate::ui::space::spacing(cx).control_gap)
                        .child(
                            Button::new("tunnel-delete-confirm")
                                .label("Delete")
                                .danger()
                                .small()
                                .on_click(move |_event, _window, cx| {
                                    let _ = weak_confirm
                                        .update(cx, |editor, cx| editor.confirm_delete(cx));
                                }),
                        )
                        .child(
                            Button::new("tunnel-delete-cancel")
                                .label("Keep")
                                .outline()
                                .small()
                                .on_click(move |_event, _window, cx| {
                                    let _ = weak_cancel
                                        .update(cx, |editor, cx| editor.cancel_delete(cx));
                                }),
                        ),
                )
        });

        let weak_save = cx.weak_entity();
        let weak_test = cx.weak_entity();
        let weak_cancel_pane = cx.weak_entity();
        let weak_delete = cx.weak_entity();

        div()
            .flex()
            .flex_col()
            .gap(crate::ui::space::spacing(cx).section_gap)
            .p(crate::ui::space::spacing(cx).panel_inset)
            .track_focus(&self.focus_handle)
            .child(div().text_base().font_semibold().child(if is_new {
                "New Tunnel"
            } else {
                "Edit Tunnel"
            }))
            .children(self.general_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(error.clone())
            }))
            .child(self.render_kind_switch(cx))
            .child(field("Name", &self.name, None))
            .when(is_ssh, |pane| {
                pane.child(field("Host", &self.host, host_error))
                    .child(field("User", &self.user, user_error))
                    .child(field("Port", &self.port, port_error))
                    .child(field(
                        "Jump hosts (comma-separated, optional)",
                        &self.jump_hosts,
                        None,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Authentication"),
                            )
                            .child(auth_button),
                    )
                    .children(key_field)
            })
            .when(!is_ssh, |pane| pane.child(self.render_command_form(cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("tunnel-test")
                            .label("Test")
                            .outline()
                            .small()
                            .disabled(self.testing)
                            .on_click(move |_event, _window, cx| {
                                let _ = weak_test.update(cx, |editor, cx| editor.run_test(cx));
                            }),
                    )
                    .children(test_status),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("tunnel-save")
                                    .label("Save")
                                    .primary()
                                    .small()
                                    .on_click(move |_event, window, cx| {
                                        let _ = weak_save
                                            .update(cx, |editor, cx| editor.save(window, cx));
                                    }),
                            )
                            .child(
                                Button::new("tunnel-cancel")
                                    .label("Cancel")
                                    .ghost()
                                    .small()
                                    .on_click(move |_event, _window, cx| {
                                        let _ = weak_cancel_pane
                                            .update(cx, |editor, cx| editor.cancel(cx));
                                    }),
                            ),
                    )
                    .children((!is_new).then(|| {
                        Button::new("tunnel-delete")
                            .label("Delete")
                            .danger()
                            .small()
                            .on_click(move |_event, _window, cx| {
                                let _ =
                                    weak_delete.update(cx, |editor, cx| editor.request_delete(cx));
                            })
                    })),
            )
            .children(delete_confirm)
    }
}

/// The tunnel delete prompt's question, naming the contexts that fall back.
pub(super) fn delete_confirm_text(bound_contexts: &[String]) -> ConfirmText {
    let question = ConfirmText::from("Delete this tunnel? ");
    match bound_contexts {
        [] => question.text("No contexts are bound to it."),
        [_] => question
            .text("This context will fall back to a direct connection: ")
            .names(bound_contexts),
        _ => question
            .text("These contexts will fall back to a direct connection: ")
            .names(bound_contexts),
    }
}
