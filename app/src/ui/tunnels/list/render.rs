//! Owns the Tunnels window's rendered layout: the header and New Tunnel control,
//! each tunnel's row (name, usage, running state, Edit), the stale-bindings section
//! with its per-row Remove, and the embedded editor pane.

use super::*;

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

        let rows =
            self.tunnels.iter().map(|(id, tunnel)| {
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
                            .child(div().text_sm().font_medium().child(tunnel.name.clone()))
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                format!(
                                    "{}@{}:{}",
                                    tunnel.bastion_user, tunnel.bastion_host, tunnel.bastion_port
                                ),
                            )),
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
                .gap_2()
                .pt_2()
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
            .gap_3()
            .p_4()
            .bg(theme.background)
            .track_focus(&self.focus_handle)
            .child(header)
            .child(div().flex().flex_col().gap_1().children(rows))
            .children(stale_section)
            .children(self.editor.clone().map(|editor| {
                div()
                    .border_t_1()
                    .border_color(theme.border)
                    .pt_3()
                    .child(editor)
            }))
    }
}
