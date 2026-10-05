//! Deleting one object from wherever it is shown (`k9s-remaining-keybindings`):
//! the Pods list, any kind's list, the object and pod detail panels. Each asks
//! through the same confirmation, naming the object, and sends the same
//! request (`resource_actions::delete`); a force kill skips the question.
//!
//! Kind-agnostic and panel-agnostic: a [`DeleteTarget`] says what to delete,
//! and the caller's `on_done` hears how it went - a panel shows a refusal, a
//! success needs nothing, since the watch drops the row or marks the detail
//! deleted. Bulk delete builds on [`send_delete`] for each selected object.
//!
//! Owns the question and the request's round trip, not which panel offers the
//! command or how it shows a refusal ([`refusal`]).

use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::discovery::DiscoveredKind;
use crate::k8s::resource::resource_actions::{self, ActionFailure};
use crate::ui::confirm_dialog::{self, Confirmation};
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::*;
use std::rc::Rc;

pub(crate) mod refusal;

/// The Delete confirmation's id prefix: its buttons are `delete-cancel` and
/// `delete-confirm`.
pub(crate) const DELETE_ID_PREFIX: &str = "delete";

/// One object to delete: which context, kind, namespace and name.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeleteTarget {
    pub(crate) context_name: String,
    pub(crate) kind: DiscoveredKind,
    /// `None` for a cluster-scoped object.
    pub(crate) namespace: Option<String>,
    pub(crate) name: String,
}

/// How a delete went, for whoever asked.
pub(crate) type OnDeleted = Rc<dyn Fn(Result<(), ActionFailure>, &mut App)>;

impl DeleteTarget {
    /// The confirmation's question: the kind, the quoted name, and its
    /// namespace. A pod's adds that its controller may replace it.
    pub(crate) fn question(&self) -> ConfirmText {
        let kind = &self.kind.gvk.kind;
        let question = ConfirmText::from(format!("Delete {kind} ").as_str()).name(&self.name);
        let question = match &self.namespace {
            Some(namespace) => question.text(" in ").name(namespace).text("?"),
            None => question.text("?"),
        };
        if self.kind == DiscoveredKind::pods() {
            question.text(" A controller that owns it may start a replacement.")
        } else {
            question
        }
    }

    /// What a refusal banner says was asked: "Delete Secret db-password".
    pub(crate) fn action(&self, force: bool) -> String {
        let verb = if force { "Kill" } else { "Delete" };
        format!("{verb} {} {}", self.kind.gvk.kind, self.name)
    }
}

/// Asks before deleting `target` over `connection`, then deletes it if the
/// user confirms - telling `on_done` how it went.
pub(crate) fn confirm_delete(
    target: DeleteTarget,
    connection: Entity<ClusterConnection>,
    on_done: OnDeleted,
    window: &mut Window,
    cx: &mut App,
) {
    let confirmation = Confirmation {
        title: format!("Delete {}?", target.kind.gvk.kind).into(),
        body: target.question(),
        confirm: "Delete".into(),
        id_prefix: DELETE_ID_PREFIX,
    };
    confirm_dialog::open(
        confirmation,
        move |_window, cx| send_delete(target.clone(), &connection, false, on_done.clone(), cx),
        window,
        cx,
    );
}

/// Deletes `target` over `connection` now - `force` for a zero-grace kill -
/// and tells `on_done` how it went once the cluster answers. A connection
/// that isn't up refuses at once. The client is read here, at send time, so
/// a confirmation left open across a reconnect uses the current one.
/// `on_done` never runs inside this call.
pub(crate) fn send_delete(
    target: DeleteTarget,
    connection: &Entity<ClusterConnection>,
    force: bool,
    on_done: OnDeleted,
    cx: &mut App,
) {
    let ConnectionState::Connected(client) = &connection.read(cx).state else {
        // Deferred, like the answer from the cluster: the caller is usually
        // mid-update in its own action handler, and `on_done` updates it.
        let failure = ActionFailure {
            message: format!("{} is not connected.", target.context_name),
            detail: String::new(),
        };
        cx.defer(move |cx| on_done(Err(failure), cx));
        return;
    };
    let client = client.clone();
    let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let result = resource_actions::delete(
            client,
            &target.kind,
            &target.name,
            target.namespace.as_deref(),
            force,
        )
        .await;
        let _ = tx.send(result).await;
    });
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |result| cx.update(|cx| on_done(result, cx))).await;
    })
    .detach();
}

#[cfg(test)]
mod tests;
