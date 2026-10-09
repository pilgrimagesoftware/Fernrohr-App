//! Owns the editor's body: the field list with inline errors, the auth choice and its
//! private-key field, and Test with its status. The dialog around it
//! (`list::editor_dialog`) owns the title and Save/Cancel/Delete.
//!
//! The body scrolls itself, each section a child of the scroll area wrapped in a
//! focus handle of its own (never a tab stop), so the section holding focus - the
//! field Tab just reached - is scrolled into view; gpui doesn't follow focus.

use super::*;
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::component::scroll::ScrollableElement as _;

impl Render for TunnelEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

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

        let weak_test = cx.weak_entity();

        let mut sections: Vec<AnyElement> = Vec::new();
        if let Some(error) = &self.general_error {
            sections.push(
                div()
                    .text_sm()
                    .text_color(theme.danger)
                    .child(error.clone())
                    .into_any_element(),
            );
        }
        sections.push(self.render_kind_switch(cx).into_any_element());
        sections.push(field("Name", &self.name, None).into_any_element());
        if self.kind == TunnelKind::Ssh {
            sections.push(field("Host", &self.host, host_error).into_any_element());
            sections.push(field("User", &self.user, user_error).into_any_element());
            sections.push(field("Port", &self.port, port_error).into_any_element());
            sections.push(
                field(
                    "Jump hosts (comma-separated, optional)",
                    &self.jump_hosts,
                    None,
                )
                .into_any_element(),
            );
            sections.push(
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
                    .child(auth_button)
                    .into_any_element(),
            );
            sections.extend(key_field.map(IntoElement::into_any_element));
        } else if self.kind == TunnelKind::Command {
            sections.extend(self.render_command_form(cx));
        } else {
            sections.extend(self.render_manual_form(cx));
        }
        // A manual tunnel starts nothing, so it has nothing to test.
        let testable = self.kind != TunnelKind::Manual;
        sections.extend(testable.then(|| {
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
                .children(test_status)
                .into_any_element()
        }));

        while self.sections.len() < sections.len() {
            self.sections.push(cx.focus_handle());
        }
        self.reveal_focused(window, cx);

        div()
            .id("tunnel-editor-body")
            .size_full()
            .flex()
            .flex_col()
            .gap(crate::ui::space::spacing(cx).section_gap)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .vertical_scrollbar(&self.scroll)
            .track_focus(&self.focus_handle)
            .children(
                sections
                    .into_iter()
                    .zip(&self.sections)
                    .map(|(section, focus)| div().track_focus(focus).child(section)),
            )
    }
}

impl TunnelEditor {
    /// Scrolls the section holding focus into view once, when focus reaches it, so
    /// the user can still scroll away from it after.
    fn reveal_focused(&mut self, window: &Window, cx: &App) {
        let focused = self
            .sections
            .iter()
            .position(|section| section.contains_focused(window, cx));
        if focused != self.revealed
            && let Some(ix) = focused
        {
            self.scroll.scroll_to_item(ix);
        }
        self.revealed = focused;
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
