//! The command-tunnel half of the editor (`command-tunnels` 4.1-4.2): the kind and
//! mode switches, reading the command form's fields into a [`CommandTunnelConfig`],
//! testing it, and drawing it.
//!
//! The switches are buttons, so they are tab stops that Space or Enter press, like
//! the rest of the pane. Tab leaves the multi-line command rather than indenting it. Switching kind only changes which form shows and which kind
//! Save writes; both forms keep what was typed.

use super::*;
use gpui_kit::component::input::{IndentInline, OutdentInline};

/// One line on what each mode means, under the mode switch.
fn mode_help(mode: CommandTunnelMode) -> &'static str {
    match mode {
        CommandTunnelMode::Proxy => {
            "The local port is an HTTP proxy. Bound contexts keep their API server address \
             and send their traffic through it."
        }
        CommandTunnelMode::Forward => {
            "The local port reaches the API server directly. Bound contexts connect to it on \
             127.0.0.1, checking the certificate against the real host."
        }
    }
}

const PLAIN_TEXT_HINT: &str = "Stored in plain text in tunnels.toml, so leave credentials to the \
     tool's own login. Runs without a shell (no pipes or $VARS); {port} becomes the local port. \
     Run it once in a terminal first, so it never stops to ask anything.";

/// The startup timeout field's debug selector, for tests to find where it draws.
pub(in crate::ui::tunnels) const TIMEOUT_FIELD_SELECTOR: &str = "tunnel-timeout-field";

impl TunnelEditor {
    pub(super) fn set_kind(&mut self, kind: TunnelKind, cx: &mut Context<Self>) {
        self.kind = kind;
        self.field_errors.clear();
        self.test_result = None;
        cx.notify();
    }

    pub(super) fn set_mode(&mut self, mode: CommandTunnelMode, cx: &mut Context<Self>) {
        self.mode = mode;
        cx.notify();
    }

    /// The command form's values. An unparsable port reads as `Some(0)` and an
    /// unparsable timeout as `0`, so the store's validation names the field rather
    /// than the typo silently becoming "no port" or the default.
    pub(super) fn command_config(&self, cx: &App) -> CommandTunnelConfig {
        let port_text = self.local_port.read(cx).value().trim().to_string();
        let local_port = (!port_text.is_empty()).then(|| port_text.parse::<u16>().unwrap_or(0));
        let startup_timeout_secs = self
            .startup_timeout
            .read(cx)
            .value()
            .trim()
            .parse()
            .unwrap_or(0);
        CommandTunnelConfig {
            command_line: self.command_line.read(cx).value().to_string(),
            mode: self.mode,
            local_port,
            startup_timeout_secs,
        }
    }

    /// Runs the command until it is ready (or fails), then stops it - see
    /// `tunnel::command::test_command`. Nothing is left running either way.
    pub(super) fn run_command_test(&mut self, cx: &mut Context<Self>) {
        let config = self.command_config(cx);
        self.testing = true;
        self.test_result = None;
        cx.notify();
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let result = crate::tunnel::command::test_command(&config).await;
            let _ = tx.send(result).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, move |result| {
                let _ = this.update(cx, |editor, cx| {
                    editor.testing = false;
                    editor.test_result = Some(result);
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }

    /// The SSH / Command switch at the top of the pane.
    pub(super) fn render_kind_switch(&self, cx: &Context<Self>) -> impl IntoElement {
        let choice = |id: &'static str, label: &'static str, kind: TunnelKind| {
            let weak = cx.weak_entity();
            let button = Button::new(id).label(label).small();
            let button = if self.kind == kind {
                button.primary()
            } else {
                button.outline()
            };
            button.on_click(move |_event, _window, cx| {
                let _ = weak.update(cx, |editor, cx| editor.set_kind(kind, cx));
            })
        };
        div()
            .flex()
            .gap_1()
            .child(choice("tunnel-kind-ssh", "SSH tunnel", TunnelKind::Ssh))
            .child(choice(
                "tunnel-kind-command",
                "Command tunnel",
                TunnelKind::Command,
            ))
    }

    /// The command form's sections, in order: the command, its mode with a line of
    /// help, the optional fixed port and the startup timeout, each with its
    /// validation message. Separate, so the editor can scroll each into view.
    pub(super) fn render_command_form(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme().clone();
        let error = |field: TunnelFieldError, message: &'static str| {
            self.field_errors
                .contains(&field)
                .then(|| div().text_xs().text_color(theme.danger).child(message))
        };
        let label = |text: &'static str| {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(text)
        };
        let mode_choice = |id: &'static str, text: &'static str, mode: CommandTunnelMode| {
            let weak = cx.weak_entity();
            let button = Button::new(id).label(text).small();
            let button = if self.mode == mode {
                button.primary()
            } else {
                button.outline()
            };
            button.on_click(move |_event, _window, cx| {
                let _ = weak.update(cx, |editor, cx| editor.set_mode(mode, cx));
            })
        };
        vec![
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(label("Command"))
                // A multi-line input takes Tab to indent; a command line never
                // wants a tab, so Tab and Shift-Tab move focus on instead, and
                // the fields after it stay reachable from the keyboard.
                .child(
                    div()
                        .capture_action(|_: &IndentInline, window, cx| {
                            window.focus_next(cx);
                            cx.stop_propagation();
                        })
                        .capture_action(|_: &OutdentInline, window, cx| {
                            window.focus_prev(cx);
                            cx.stop_propagation();
                        })
                        .child(Textarea::new(&self.command_line).w_full().h(px(88.))),
                )
                .children(error(
                    TunnelFieldError::EmptyCommand,
                    "Enter the command to run.",
                ))
                .children(error(
                    TunnelFieldError::UnbalancedQuotes,
                    "The command has an unclosed quote.",
                ))
                .children(error(
                    TunnelFieldError::NoPortPlaceholder,
                    "Add {port} to the command, or set a fixed local port.",
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(PLAIN_TEXT_HINT),
                )
                .into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(label("Mode"))
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(mode_choice(
                            "tunnel-mode-proxy",
                            "Proxy",
                            CommandTunnelMode::Proxy,
                        ))
                        .child(mode_choice(
                            "tunnel-mode-forward",
                            "Forward",
                            CommandTunnelMode::Forward,
                        )),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(mode_help(self.mode)),
                )
                .into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(label("Local port (optional; the command must then use it)"))
                .child(Input::new(&self.local_port))
                .children(error(
                    TunnelFieldError::InvalidLocalPort,
                    "Local port must be between 1 and 65535.",
                ))
                .into_any_element(),
            div()
                .flex()
                .flex_col()
                .gap_1()
                .debug_selector(|| TIMEOUT_FIELD_SELECTOR.into())
                .child(label("Startup timeout (seconds)"))
                .child(Input::new(&self.startup_timeout))
                .children(error(
                    TunnelFieldError::InvalidTimeout,
                    "Startup timeout must be a positive number of seconds.",
                ))
                .into_any_element(),
        ]
    }
}

#[cfg(test)]
mod tests;
