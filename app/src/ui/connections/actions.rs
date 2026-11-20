//! What Open in Window, Disconnect and End Agent's Hold do to a row. Each acts on
//! the selected row; a row's icon button selects its row first, so the click and
//! the key do the same thing.
//!
//! End Agent's Hold ends every agent's hold on the row's context, not one
//! agent's: a row with a single agent (the common case) is exactly that agent's,
//! and with several there is no sub-selection to keep keyboard-operable. Each
//! agent's later request for the context then needs approval, as for a new one.

use super::rows::{ConnectionRow, HolderLabel};
use super::window::ConnectionsWindow;
use crate::k8s::cluster::session::{AgentId, ClusterRegistry};
use crate::ui::confirm_dialog::{self, Confirmation, Severity};
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::{Context, Window};

/// The Disconnect confirmation's id prefix: its buttons are
/// `connections-disconnect-cancel` and `connections-disconnect-confirm`.
pub(super) const DISCONNECT_ID_PREFIX: &str = "connections-disconnect";

/// The Disconnect question for `row`: the windows whose panels close and the
/// agents that lose the context, each named. An agent is named as it reported
/// itself, and says so.
pub(super) fn disconnect_body(row: &ConnectionRow) -> ConfirmText {
    let mut body = ConfirmText::new()
        .name(&row.context)
        .text(" will disconnect.");
    let windows = row.window_titles();
    if !windows.is_empty() {
        body = body
            .text(" It closes the panels for it in ")
            .names(&windows)
            .text(".");
    }
    let agents: Vec<&HolderLabel> = row
        .holders
        .iter()
        .filter(|holder| matches!(holder, HolderLabel::Agent { .. }))
        .collect();
    if !agents.is_empty() {
        body = body.text(" It ends the hold of ");
        for (index, agent) in agents.into_iter().enumerate() {
            if index > 0 {
                body = body.text(", ");
            }
            body = match agent {
                HolderLabel::Agent {
                    name: Some(name), ..
                } => body.name(name).text(" (self-reported)"),
                _ => body.text("an unnamed agent"),
            };
        }
        body = body.text(".");
    }
    body
}

impl ConnectionsWindow {
    /// Open in Window: adds the selected context to the frontmost workspace
    /// window - or opens one - reusing the connection, with no approval.
    pub(super) fn open_in_window(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self.selected_row(cx) {
            crate::util::shell::open_context_in_window(row.context, cx);
        }
    }

    /// Disconnect: asks first, naming the windows and agents it affects, then
    /// releases every holder. Recoverable: Enter confirms, Escape cancels.
    pub(super) fn disconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row(cx) else {
            return;
        };
        let context = row.context.clone();
        confirm_dialog::open(
            Confirmation {
                title: "Disconnect Context?".into(),
                body: disconnect_body(&row),
                confirm: "Disconnect".into(),
                id_prefix: DISCONNECT_ID_PREFIX,
                severity: Severity::Recoverable,
            },
            move |_window, cx| crate::util::shell::disconnect_everywhere(&context, cx),
            window,
            cx,
        );
    }

    /// End Agent's Hold from the keyboard: ends the hold of the selected row's
    /// one agent, without asking; with several agents, asks which one
    /// ([`super::agent_picker`]) so only that agent's hold ends. The connection
    /// closes if nothing else holds it.
    pub(super) fn end_agent_hold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row(cx) else {
            return;
        };
        let mut agents = row.agent_labels();
        match agents.len() {
            0 => {}
            1 => {
                let (agent, _) = agents.remove(0);
                ClusterRegistry::release(cx, &row.context, agent);
            }
            _ => super::agent_picker::open(row.context, agents, window, cx),
        }
    }

    /// One agent's End button: ends only `agent`'s hold on `context`.
    pub(super) fn end_one(&mut self, context: &str, agent: AgentId, cx: &mut Context<Self>) {
        ClusterRegistry::release(cx, context, agent);
    }
}
