//! End Agent's Hold when more than one agent holds the selected context
//! (`connections-window`: End one agent's hold): a dialog with one button per
//! agent, so the user ends exactly the one they choose and the others keep the
//! context. Focus opens on the first agent; Tab moves between the buttons,
//! Enter or Space presses the focused one, and Escape (or Cancel) ends nothing.
//!
//! Agents are named as they report themselves - a label, never an identity -
//! and each button acts on its own [`AgentId`], never on the name.

use crate::k8s::cluster::session::{AgentId, ClusterRegistry};
use crate::ui::confirm_dialog::{cancel_key, confirm_key, focus_first_stop_in};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::{Confirm, DialogFooter};
use gpui_kit::component::v_flex;
use gpui_kit::*;
use std::rc::Rc;

/// The picker's Cancel button.
pub(super) const CANCEL_ID: &str = "connections-agent-picker-cancel";

/// The picker button for the agent at `index` in the row's agent order.
pub(super) fn agent_button_id(index: usize) -> SharedString {
    format!("connections-agent-picker-{index}").into()
}

/// Opens the picker over `context`'s agents, each with its label.
pub(super) fn open(
    context: String,
    agents: Vec<(AgentId, String)>,
    window: &mut Window,
    cx: &mut App,
) {
    let context: Rc<str> = context.into();
    window.open_dialog(cx, move |dialog, window, _cx| {
        let buttons = agents.iter().enumerate().map(|(index, (agent, label))| {
            let agent = *agent;
            let (on_enter, on_click) = (Rc::clone(&context), Rc::clone(&context));
            // The button's parent hears the dialog's Enter while the button
            // has focus, so Enter presses the focused agent's button.
            div()
                .on_action(move |_: &Confirm, window, cx| {
                    window.close_dialog(cx);
                    ClusterRegistry::release(cx, &on_enter, agent);
                })
                .child(
                    Button::new(agent_button_id(index))
                        .label(label.clone())
                        .child(confirm_key(window))
                        .on_click(move |_event, window, cx| {
                            window.close_dialog(cx);
                            ClusterRegistry::release(cx, &on_click, agent);
                        }),
                )
        });
        dialog
            .title("End Which Agent's Hold?")
            .child(format!(
                "More than one agent uses {context}. Choose the one to end; the others keep it."
            ))
            .child(v_flex().gap_1().mt_2().children(buttons))
            .on_cancel(|_event, _window, _cx| true)
            // Enter with no agent focused ends nothing.
            .on_ok(|_event, _window, _cx| false)
            .footer(
                DialogFooter::new().child(
                    div()
                        .on_action(|_: &Confirm, window, cx| window.close_dialog(cx))
                        .child(
                            Button::new(CANCEL_ID)
                                .label("Cancel")
                                .child(cancel_key(window))
                                .on_click(|_event, window, cx| window.close_dialog(cx)),
                        ),
                ),
            )
    });
    if let Some(dialog) = window.focused(cx) {
        window.on_next_frame(move |window, cx| {
            if dialog.is_focused(window) {
                focus_first_stop_in(&dialog, window, cx);
            }
        });
    }
}
