//! `Render for ClusterPicker`: assembles the backdrop, logo, and card
//! ([`super::layout`]) around the context list built from [`super::rows::context_row`]
//! and the controls in [`super::rows`], wiring `Command`'s `on_select`/`on_confirm`
//! into the selection logic in [`super::interaction`].

use super::*;

impl Render for ClusterPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        // The logo sits outside the card so it reads as app branding rather than
        // as part of the command-palette chrome the card deliberately mimics.
        let backdrop = |content: AnyElement| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.background)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_6()
                        .child(logo())
                        .child(content),
                )
        };

        let contexts = match &self.contexts {
            Ok(contexts) if !contexts.is_empty() => contexts.clone(),
            Ok(_) => {
                return backdrop(
                    card(cx)
                        .child(header(cx))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("No kubeconfig contexts are available."),
                        )
                        .track_focus(&self.focus_handle)
                        .into_any_element(),
                )
                .into_any_element();
            }
            Err(error) => {
                return backdrop(
                    card(cx)
                        .child(header(cx))
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.danger)
                                .child(format!("Could not read kubeconfig: {error}")),
                        )
                        .track_focus(&self.focus_handle)
                        .into_any_element(),
                )
                .into_any_element();
            }
        };

        let this = cx.weak_entity();
        let tunnel_choices = self.tunnel_choices.clone();
        let tunnel_bindings = self.tunnel_bindings.clone();
        let selected = self.selected_context.clone();
        let items: Vec<CommandItem> = contexts
            .iter()
            .enumerate()
            .map(|(row_index, name)| {
                let bound_id = tunnel_bindings.get(name).cloned();
                CommandItem::new().label(name.clone()).child(context_row(
                    name.clone(),
                    row_index,
                    selected.as_deref() == Some(name.as_str()),
                    bound_id,
                    tunnel_choices.clone(),
                    this.clone(),
                ))
            })
            .collect();
        let command = Command::new(&self.command_state)
            .items(items)
            .placeholder("Search contexts...")
            // Keyboard navigation (arrows, or typing to filter) moves the selection;
            // hover moves only `Command`'s own highlight. Both a click and the
            // keyboard select, so Connect and `context.set_tunnel` follow either.
            .on_select({
                let this = this.clone();
                move |index_path, window, cx| {
                    if !window.last_input_was_keyboard() {
                        return;
                    }
                    let _ = this.update(cx, |this, cx| this.follow_keyboard(index_path.row, cx));
                }
            })
            .on_confirm({
                let this = this.clone();
                move |index_path, _window, cx| {
                    let _ = this.update(cx, |this, cx| this.confirm_row(index_path.row, cx));
                }
            });

        let status = self.attempt.as_ref().map(|attempt| {
            let context_name = attempt.context_name.clone();
            let (text, color) = match &attempt.connection.read(cx).state {
                ConnectionState::Connecting => (
                    format!("Connecting to {context_name}..."),
                    theme.muted_foreground,
                ),
                ConnectionState::WaitingForTunnel => (
                    format!("Waiting for tunnel to {context_name}..."),
                    theme.muted_foreground,
                ),
                ConnectionState::Failed(reason) => (
                    format!("Could not connect to {context_name}: {reason}"),
                    theme.danger,
                ),
                ConnectionState::Connected(_) => {
                    (format!("Connected to {context_name}"), theme.success)
                }
            };
            div().text_sm().text_color(color).child(text)
        });

        let connect_disabled = self.selected_context.is_none() || self.is_connect_in_flight(cx);

        backdrop(
            card(cx)
                .child(header(cx))
                .child(command)
                .children(status)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(crate::ui::picker_keys::with_key(
                            connect_button(connect_disabled, this.clone()),
                            Some(crate::ui::picker_keys::enter_key()),
                        ))
                        .child(crate::ui::picker_keys::with_key(
                            manage_tunnels_control(),
                            crate::ui::picker_keys::manage_tunnels_key(window),
                        )),
                )
                .child(crate::ui::picker_keys::key_hints(window, cx))
                .track_focus(&self.focus_handle)
                .into_any_element(),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests;
