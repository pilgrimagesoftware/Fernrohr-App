//! The dialog an agent's action waits on (`agent-mcp`: Action approval): what
//! the agent asks to do, to which objects, with which values, and Allow or
//! Cancel - the app's own confirmation dialog, in its irreversible tier for an
//! action that can't be taken back, so every key works as it does for the
//! user's own deletes (`keyboard-first.md`).
//!
//! `mcp::approval` asks through [`open`] and takes the question back with
//! [`withdraw`] when the agent's call goes away first. One question is open at
//! a time; the gate there sees to that.

use crate::mcp::approval::ApprovalRequest;
use crate::ui::confirm_dialog::{self, Confirmation, Detail, Severity};
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::component::WindowExt as _;
use gpui_kit::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;

/// The dialog's buttons are `agent-approval-cancel` and `-confirm`.
pub(crate) const ID_PREFIX: &str = "agent-approval";

/// An open question: where its dialog is, and which one it is.
#[derive(Debug)]
pub(crate) struct Question {
    window: AnyWindowHandle,
    id: u64,
}

/// The question whose dialog is open now, if any.
#[derive(Default)]
struct OpenQuestion(Option<u64>);

impl Global for OpenQuestion {}

/// Shows `request` in the frontmost main window, raising it, and answers
/// `reply` with the user's choice: `true` to allow. `None` when the app has no
/// main window to ask in.
pub(crate) fn open(
    request: ApprovalRequest,
    reply: oneshot::Sender<bool>,
    cx: &mut App,
) -> Option<Question> {
    let window = asking_window(cx)?;
    open_in(window, request, reply, cx)
}

/// [`open`], in `window`.
fn open_in(
    window: AnyWindowHandle,
    request: ApprovalRequest,
    reply: oneshot::Sender<bool>,
    cx: &mut App,
) -> Option<Question> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let reply = Rc::new(RefCell::new(Some(reply)));
    let answer = move |allow: bool, cx: &mut App| {
        if let Some(reply) = reply.borrow_mut().take() {
            let _ = reply.send(allow);
        }
        if cx
            .try_global::<OpenQuestion>()
            .is_some_and(|open| open.0 == Some(id))
        {
            cx.set_global(OpenQuestion(None));
        }
    };
    let (allow, deny) = (answer.clone(), answer);
    let confirmation = confirmation(&request);
    let details = details(&request);
    window
        .update(cx, move |_, window, cx| {
            window.activate_window();
            confirm_dialog::open_with(
                confirmation,
                details,
                move |_window, cx| allow(true, cx),
                move |_window, cx| deny(false, cx),
                window,
                cx,
            );
        })
        .ok()?;
    cx.set_global(OpenQuestion(Some(id)));
    Some(Question { window, id })
}

/// Closes `question`'s dialog if it is still the one open, unanswered.
pub(crate) fn withdraw(question: Question, cx: &mut App) {
    if !cx
        .try_global::<OpenQuestion>()
        .is_some_and(|open| open.0 == Some(question.id))
    {
        return;
    }
    cx.set_global(OpenQuestion(None));
    let _ = question.window.update(cx, |_, window, cx| {
        if window.has_active_dialog(cx) {
            window.close_dialog(cx);
        }
    });
}

/// The window to ask in: the active one if it is a main window, else the
/// first main window open.
fn asking_window(cx: &App) -> Option<AnyWindowHandle> {
    let is_main = |handle: &AnyWindowHandle| crate::util::shell::is_main_window(*handle, cx);
    cx.active_window()
        .filter(is_main)
        .or_else(|| cx.windows().into_iter().find(is_main))
}

fn confirmation(request: &ApprovalRequest) -> Confirmation {
    Confirmation {
        title: request.title.clone().into(),
        body: ConfirmText::from("An agent connected to Fernrohr asks to ")
            .text(&request.confirm.to_lowercase())
            .text(" in ")
            .name(&request.context)
            .text(". Nothing changes unless you allow it."),
        confirm: request.confirm.clone().into(),
        id_prefix: ID_PREFIX,
        severity: if request.irreversible {
            Severity::Irreversible
        } else {
            Severity::Recoverable
        },
    }
}

/// The rows under the question: where, what, which objects - one row each -
/// and the action's own values.
pub(crate) fn details(request: &ApprovalRequest) -> Vec<Detail> {
    let row = |label: &str, value: &str| Detail {
        label: label.to_string().into(),
        value: value.to_string().into(),
    };
    let mut rows = vec![
        row("Agent tool", &request.tool),
        row("Context", &request.context),
        row("Namespace", &request.namespace),
        row("Kind", &request.kind),
    ];
    for (index, target) in request.targets.iter().enumerate() {
        let label = match (index, request.targets.len()) {
            (0, 1) => "Name",
            (0, _) => "Names",
            _ => "",
        };
        rows.push(row(label, target));
    }
    rows.extend(
        request
            .parameters
            .iter()
            .map(|(label, value)| row(label, value)),
    );
    rows
}

#[cfg(test)]
mod tests;
