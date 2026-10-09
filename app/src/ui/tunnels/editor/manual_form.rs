//! The manual-tunnel half of the editor (`manual-confirmation-tunnels` 1.2): the
//! instruction the prompt shows, and whether an already-reachable API server skips
//! the prompt.
//!
//! The reachability choice is two buttons, like the command form's mode switch, so
//! each is a tab stop that Space or Enter presses. Switching kind keeps what was
//! typed here, as the other forms do.

use super::*;

/// Under the reachability choice: what each setting means.
fn skip_help(skip_when_reachable: bool) -> &'static str {
    if skip_when_reachable {
        "Before asking, Fernrohr tries a quick TCP connect to the context's API server, and \
         connects without a prompt when it answers. A public endpoint answers without your \
         VPN, so choose Always prompt for one."
    } else {
        "Every connection that needs this tunnel asks first, even when the API server \
         already answers."
    }
}

impl TunnelEditor {
    pub(super) fn set_skip_when_reachable(&mut self, skip: bool, cx: &mut Context<Self>) {
        self.skip_when_reachable = skip;
        cx.notify();
    }

    /// The manual form's values. A blank message is no message.
    pub(super) fn manual_config(&self, cx: &App) -> ManualTunnelConfig {
        let message = self.manual_message.read(cx).value().trim().to_string();
        ManualTunnelConfig {
            message: (!message.is_empty()).then_some(message),
            skip_when_reachable: self.skip_when_reachable,
        }
    }

    /// The manual form's sections: the instruction, then the reachability choice.
    pub(super) fn render_manual_form(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme().clone();
        let label = |text: &'static str| {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(text)
        };
        let choice = |id: &'static str, text: &'static str, skip: bool| {
            let weak = cx.weak_entity();
            let button = Button::new(id).label(text).small();
            let button = if self.skip_when_reachable == skip {
                button.primary()
            } else {
                button.outline()
            };
            button.on_click(move |_event, _window, cx| {
                let _ = weak.update(cx, |editor, cx| editor.set_skip_when_reachable(skip, cx));
            })
        };
        vec![
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(label("Instruction (optional)"))
                .child(Input::new(&self.manual_message))
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    "Shown each time a connection waits on this tunnel. Fernrohr \
                             starts nothing: you bring the network path up, then choose \
                             Proceed.",
                ))
                .into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(label("When the API server already answers"))
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(choice("tunnel-manual-skip", "Skip the prompt", true))
                        .child(choice("tunnel-manual-always", "Always prompt", false)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(skip_help(self.skip_when_reachable)),
                )
                .into_any_element(),
        ]
    }
}
