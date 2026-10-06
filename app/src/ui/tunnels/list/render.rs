//! Owns the Tunnels window's rendered layout: the header and New Tunnel control,
//! each tunnel's row (name, usage, running state, Edit), the stale-bindings section
//! with its per-row Remove, and the embedded editor pane.

use super::*;

/// The row's kind badge.
pub(super) fn kind_label(kind: TunnelKind) -> &'static str {
    match kind {
        TunnelKind::Ssh => "SSH",
        TunnelKind::Command => "Command",
    }
}

/// The row's second line: where an SSH tunnel goes, or a command tunnel's command -
/// its first line, shortened - and mode.
pub(super) fn tunnel_summary(tunnel: &TunnelConfig) -> String {
    match tunnel.kind {
        TunnelKind::Ssh => format!(
            "{}@{}:{}",
            tunnel.bastion_user, tunnel.bastion_host, tunnel.bastion_port
        ),
        TunnelKind::Command => {
            let first = tunnel
                .command
                .command_line
                .lines()
                .next()
                .unwrap_or_default();
            let first = first.trim().trim_end_matches('\\').trim_end();
            let mut command: String = first.chars().take(60).collect();
            if first.chars().count() > 60 || tunnel.command.command_line.trim().lines().count() > 1
            {
                command.push('\u{2026}');
            }
            let mode = match tunnel.command.mode {
                CommandTunnelMode::Proxy => "proxy",
                CommandTunnelMode::Forward => "forward",
            };
            format!("{command} ({mode})")
        }
    }
}

fn usage_label(count: usize) -> String {
    match count {
        0 => "Unused".to_string(),
        1 => "In use by 1 context".to_string(),
        n => format!("In use by {n} contexts"),
    }
}

impl Render for TunnelsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        let weak_new = cx.weak_entity();
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(div().text_lg().font_semibold().child("Tunnels"))
            .child(
                Button::new("tunnels-new")
                    .label("New Tunnel")
                    .icon(IconName::Plus)
                    .primary()
                    .small()
                    .on_click(move |_event, window, cx| {
                        let _ = weak_new.update(cx, |this, cx| this.open_create(window, cx));
                    }),
            );

        let rows = self.tunnels.iter().map(|(id, tunnel)| {
            let usage = self.usage.get(id).copied().unwrap_or(0);
            let running = self.is_running(id);
            let weak_edit = cx.weak_entity();
            let id_for_edit = id.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .py_1()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(div().text_sm().font_medium().child(tunnel.name.clone()))
                                .child({
                                    let selector = format!("tunnel-kind-{id}");
                                    div()
                                        .px_1()
                                        .rounded_sm()
                                        .border_1()
                                        .border_color(theme.border)
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .debug_selector(move || selector.clone())
                                        .child(kind_label(tunnel.kind))
                                }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(tunnel_summary(tunnel)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(usage_label(usage)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(if running {
                                    theme.success
                                } else {
                                    theme.muted_foreground
                                })
                                .child(if running { "Running" } else { "Idle" }),
                        )
                        .child(
                            Button::new(format!("tunnels-edit-{id}"))
                                .label("Edit")
                                .outline()
                                .xsmall()
                                .on_click(move |_event, window, cx| {
                                    let _ = weak_edit.update(cx, |this, cx| {
                                        this.open_edit(id_for_edit.clone(), window, cx)
                                    });
                                }),
                        ),
                )
        });

        let stale_section = (!self.stale.is_empty()).then(|| {
            let rows = self.stale.iter().map(|(context, tunnel_id)| {
                let weak_remove = cx.weak_entity();
                let context_for_remove = context.clone();
                let tunnel_name = self
                    .tunnels
                    .iter()
                    .find(|(id, _)| id == tunnel_id)
                    .map(|(_, tunnel)| tunnel.name.clone())
                    .unwrap_or_else(|| tunnel_id.clone());
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .child(format!("{context} \u{2192} {tunnel_name}")),
                    )
                    .child(
                        Button::new(format!("tunnels-stale-remove-{context}"))
                            .label("Remove")
                            .outline()
                            .xsmall()
                            .on_click(move |_event, _window, cx| {
                                let _ = weak_remove.update(cx, |this, cx| {
                                    this.remove_stale(context_for_remove.clone(), cx)
                                });
                            }),
                    )
            });
            div()
                .flex()
                .flex_col()
                .gap(crate::ui::space::spacing(cx).control_gap)
                .pt(crate::ui::space::spacing(cx).section_gap)
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(theme.danger)
                        .child("Stale bindings"),
                )
                .children(rows)
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(crate::ui::space::spacing(cx).section_gap)
            .p(crate::ui::space::spacing(cx).panel_inset)
            .bg(theme.background)
            .track_focus(&self.focus_handle)
            .child(header)
            .child(div().flex().flex_col().gap_1().children(rows))
            .child(self.port_forwards_section(cx))
            .children(stale_section)
            .children(self.editor.clone().map(|editor| {
                div()
                    .border_t_1()
                    .border_color(theme.border)
                    .pt(crate::ui::space::spacing(cx).section_gap)
                    .child(editor)
            }))
    }
}

#[cfg(test)]
mod tests;
