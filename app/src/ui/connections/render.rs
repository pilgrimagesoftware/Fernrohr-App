//! What the Connections window draws: a row per connection - context name, bound
//! tunnel, the status bar's state icon and text, then each holder - with Open in
//! Window, Disconnect and End Agent's Hold as icon buttons (tooltips naming them
//! and their live keys), an empty state, and a hint row of the keys.
//!
//! Up and Down move the selection (pure cursor movement); the three actions are
//! registered commands in the window's key context, handled here. A row's button
//! selects its row, then acts, so a click and a key do the same.

use super::commands::*;
use super::rows::{ConnectionRow, HolderLabel};
use super::window::ConnectionsWindow;
use crate::k8s::cluster::context_health::Severity;
use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// What the window says when no context is connected.
pub(super) const EMPTY_TEXT: &str = "No context is connected";
/// The row note for a connection only an ended agent session still holds.
pub(super) const LINGERING_TEXT: &str = "Kept open briefly after an agent's session ended";

/// The tooltips of the row buttons.
pub(super) const OPEN_TOOLTIP: &str = "Open in window";
pub(super) const DISCONNECT_TOOLTIP: &str = "Disconnect";
pub(super) const END_HOLD_TOOLTIP: &str = "End agent's hold";

/// Element ids and debug selectors, for tests.
pub(super) const EMPTY_SELECTOR: &str = "connections-empty";
pub(super) fn row_selector(context: &str) -> String {
    format!("connections-row-{context}")
}
pub(super) fn state_selector(context: &str) -> String {
    format!("connections-state-{context}")
}
pub(super) fn tunnel_selector(context: &str) -> String {
    format!("connections-tunnel-{context}")
}
pub(super) fn holder_selector(context: &str, index: usize) -> String {
    format!("connections-holder-{context}-{index}")
}
pub(super) fn open_button_id(context: &str) -> SharedString {
    format!("connections-open-{context}").into()
}
pub(super) fn disconnect_button_id(context: &str) -> SharedString {
    format!("connections-disconnect-{context}").into()
}
/// The End button beside the agent at `index` in the row's agent order.
pub(super) fn end_hold_button_id(context: &str, index: usize) -> SharedString {
    format!("connections-end-hold-{context}-{index}").into()
}

fn severity_color(theme: &gpui_kit::component::Theme, severity: Severity) -> Hsla {
    match severity {
        Severity::Muted => theme.muted_foreground,
        Severity::Info => theme.info,
        Severity::Warning | Severity::Attention => theme.warning,
        Severity::Danger => theme.danger,
    }
}

/// One row icon button: its id, icon, the tooltip's text, and the command whose
/// live key the tooltip shows.
struct RowTool {
    id: SharedString,
    icon: IconName,
    label: &'static str,
    action: Box<dyn Action>,
}

/// A row's icon button: `icon` under a tooltip reading `label` and showing
/// `action`'s live key, running `on_click`. The tooltip is findable by
/// `icon_tooltip::selector(label)`.
fn tool(spec: RowTool, on_click: impl Fn(&mut Window, &mut App) + 'static) -> Stateful<Div> {
    let RowTool {
        id,
        icon,
        label,
        action,
    } = spec;
    div()
        .id(SharedString::from(format!("{id}-tip")))
        .tooltip(move |window, cx| {
            let action = action.boxed_clone();
            Tooltip::element(move |window, _| {
                div()
                    .debug_selector(move || crate::ui::icon_tooltip::selector(label))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(label)
                    .children(Kbd::binding_for_action(
                        &*action,
                        Some(WINDOW_CONTEXT),
                        window,
                    ))
            })
            .build(window, cx)
        })
        .child(
            Button::new(id)
                .icon(icon)
                .xsmall()
                .ghost()
                .on_click(move |_, window, cx| on_click(window, cx)),
        )
}

impl ConnectionsWindow {
    fn row(&self, row: &ConnectionRow, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme().clone();
        let space = crate::ui::space::spacing(cx);
        let context = row.context.clone();
        let selected = self.selected.as_deref() == Some(row.context.as_str());
        let tint = severity_color(&theme, row.item.severity);

        let state = {
            let selector = state_selector(&context);
            crate::ui::icon_tooltip::with_tooltip_text(
                SharedString::from(format!("{selector}-tip")),
                row.item.tooltip(),
                div()
                    .debug_selector(move || selector)
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_sm()
                    .text_color(tint)
                    .child(Icon::new(row.item.icon).size(px(12.)))
                    .child(row.item.text),
            )
        };
        let tunnel = row.tunnel.as_ref().map(|tunnel| {
            let selector = tunnel_selector(&context);
            div()
                .debug_selector(move || selector)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("[{tunnel}]"))
        });
        let mut holders = Vec::with_capacity(row.holders.len());
        let mut agent_index = 0;
        for (index, holder) in row.holders.iter().enumerate() {
            let selector = holder_selector(&context, index);
            let connected_it = matches!(
                holder,
                HolderLabel::Agent {
                    connected_it: true,
                    ..
                }
            );
            // Each agent gets its own End button, so a click ends only its hold.
            let end = match holder {
                HolderLabel::Agent { id, .. } => {
                    let (agent, ended) = (*id, context.clone());
                    let button = self.button(
                        &context,
                        RowTool {
                            id: end_hold_button_id(&context, agent_index),
                            icon: IconName::Undo2,
                            label: END_HOLD_TOOLTIP,
                            action: Box::new(ConnectionsEndAgentHold),
                        },
                        move |this, _window, cx| this.end_one(&ended, agent, cx),
                        cx,
                    );
                    agent_index += 1;
                    Some(button)
                }
                HolderLabel::Window { .. } => None,
            };
            holders.push(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .debug_selector(move || selector)
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(if connected_it {
                                format!("{} \u{00b7} connected it", holder.text())
                            } else {
                                holder.text()
                            }),
                    )
                    .children(end),
            );
        }
        let lingering = row.lingering.then(|| {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(LINGERING_TEXT)
        });

        let buttons = div()
            .flex()
            .items_center()
            .gap_1()
            .child(self.button(
                &context,
                RowTool {
                    id: open_button_id(&context),
                    icon: IconName::ExternalLink,
                    label: OPEN_TOOLTIP,
                    action: Box::new(ConnectionsOpenInWindow),
                },
                |this, _window, cx| this.open_in_window(cx),
                cx,
            ))
            .child(self.button(
                &context,
                RowTool {
                    id: disconnect_button_id(&context),
                    icon: IconName::CircleX,
                    label: DISCONNECT_TOOLTIP,
                    action: Box::new(ConnectionsDisconnect),
                },
                |this, window, cx| this.disconnect(window, cx),
                cx,
            ));

        let this = cx.weak_entity();
        let clicked = context.clone();
        let selector = row_selector(&context);
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector)
            .flex()
            .items_start()
            .justify_between()
            .gap_2()
            .px(space.panel_inset)
            .py_1()
            .rounded_md()
            .when(selected, |row| row.bg(theme.selection))
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |this, cx| this.select(&clicked, cx));
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().font_medium().child(row.context.clone()))
                            .children(tunnel)
                            .child(state),
                    )
                    .children(holders)
                    .children(lingering),
            )
            .child(buttons)
    }

    /// One of a row's icon buttons: it selects `context`'s row, then runs
    /// `act` on the window.
    fn button(
        &self,
        context: &str,
        spec: RowTool,
        act: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let this = cx.weak_entity();
        let context = context.to_string();
        tool(spec, move |window, cx| {
            let _ = this.update(cx, |this, cx| {
                this.select(&context, cx);
                act(this, window, cx);
            });
        })
    }

    fn hints(&self, window: &mut Window) -> Div {
        use crate::ui::key_hint_row::{KeyHint, live_key};
        let hint = |action: &dyn Action, key: &str, label: &'static str, window: &mut Window| {
            KeyHint::body(
                vec![live_key(action, Some(WINDOW_CONTEXT), key, window)],
                label.into(),
            )
        };
        div()
            .flex()
            .flex_wrap()
            .gap_3()
            .text_sm()
            .child(KeyHint::body(
                vec![
                    Kbd::new(Keystroke::parse("up").expect("a valid key")),
                    Kbd::new(Keystroke::parse("down").expect("a valid key")),
                ],
                "Select".into(),
            ))
            .child(hint(
                &ConnectionsOpenInWindow,
                OPEN_IN_WINDOW_KEY,
                "Open in window",
                window,
            ))
            .child(hint(
                &ConnectionsDisconnect,
                DISCONNECT_KEY,
                "Disconnect",
                window,
            ))
            .child(hint(
                &ConnectionsEndAgentHold,
                END_AGENT_HOLD_KEY,
                "End agent's hold",
                window,
            ))
    }
}

impl Render for ConnectionsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let space = crate::ui::space::spacing(cx);
        let rows = self.rows(cx);
        let list: Vec<Stateful<Div>> = rows.iter().map(|row| self.row(row, cx)).collect();
        let empty = rows.is_empty().then(|| {
            div()
                .debug_selector(|| EMPTY_SELECTOR.into())
                .p(space.panel_inset)
                .text_color(theme.muted_foreground)
                .child(EMPTY_TEXT)
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(space.control_gap)
            .p(space.panel_inset)
            .bg(theme.background)
            .key_context(WINDOW_CONTEXT)
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let delta = match event.keystroke.key.as_str() {
                    "down" if !event.keystroke.modifiers.modified() => 1,
                    "up" if !event.keystroke.modifiers.modified() => -1,
                    _ => return,
                };
                this.move_selection(delta, cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|this, _: &ConnectionsOpenInWindow, _, cx| this.open_in_window(cx)),
            )
            .on_action(cx.listener(|this, _: &ConnectionsDisconnect, window, cx| {
                this.disconnect(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ConnectionsEndAgentHold, window, cx| {
                    this.end_agent_hold(window, cx)
                }),
            )
            .child(div().text_lg().font_semibold().child("Connections"))
            .child(
                div()
                    .id("connections-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(list)
                    .children(empty),
            )
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(crate::ui::style::surface_raised(cx))
                    .child(self.hints(window)),
            )
    }
}
