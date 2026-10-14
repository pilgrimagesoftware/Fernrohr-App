//! Each kind action from start to result: Expand's and Add Taint's prompt,
//! Remove Taint's choice, the confirmation every mutation asks first, the call
//! sent off the UI thread, and its refusal shown on the panel. The prompt-led
//! ones - Expand, Add and Remove Taint - are [`super::edits`]'. Port Forward
//! runs the Services list's own flow; Open Host and Create Token change nothing
//! in the cluster, so they act at once.

use super::edits::{ask_size, ask_taint, confirm_remove};
use super::host::{KindHost, KindTarget};
use super::{taints, token};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::resource::delete_flow::refusal::Refusal;
use crate::k8s::resource::kind_actions::{self, KindAction};
use crate::k8s::resource::resource_actions::ActionFailure;
use crate::ui::confirm_dialog::{self, Confirmation, Severity};
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::*;
use std::future::Future;

/// The confirmations' id prefix: `kind-confirm-cancel`, `kind-confirm-confirm`.
pub(crate) const CONFIRM_ID_PREFIX: &str = "kind-confirm";

/// Starts `action` on `this` panel's object - nothing, if it isn't offered it.
pub(super) fn start<T: KindHost>(
    action: KindAction,
    this: &mut T,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let Some(target) = this.kind_target(cx) else {
        return;
    };
    run_on(action, target, this, window, cx);
}

/// Runs `action` on `target` - the selected object's, or a row menu's own,
/// captured when the menu opened - nothing, if it isn't offered it.
pub(crate) fn run_on<T: KindHost>(
    action: KindAction,
    target: KindTarget,
    this: &mut T,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    if !target.offered().contains(&action) {
        return;
    }
    this.show_refusal(None, cx);
    let panel = cx.weak_entity();
    match action {
        KindAction::ForwardService => crate::k8s::resource::port_forwarding::forward_service(
            target.context_name.clone(),
            &target.connection,
            target.namespace().to_string(),
            target.name.clone(),
            window,
            cx,
        ),
        KindAction::OpenHost => match target.facts.host.as_deref().map(host_url) {
            Some(Ok(url)) => cx.open_url(&url),
            Some(Err(message)) => {
                let failure = ActionFailure {
                    message,
                    detail: String::new(),
                };
                refuse(&panel, format!("Open {}", target.noun()), failure, cx);
            }
            None => {}
        },
        KindAction::CreateToken => token::create(target, panel, window, cx),
        KindAction::TriggerNow => trigger_now(target, panel, window, cx),
        KindAction::Suspend | KindAction::Resume => {
            set_suspended(target, action == KindAction::Suspend, panel, window, cx)
        }
        KindAction::PauseRollout | KindAction::ResumeRollout => set_paused(
            target,
            action == KindAction::PauseRollout,
            panel,
            window,
            cx,
        ),
        KindAction::Expand => ask_size(target, panel, window, cx),
        KindAction::AddTaint => ask_taint(target, panel, window, cx),
        KindAction::RemoveTaint => taints::choose(
            target.facts.taints.clone(),
            move |taint, window, cx| {
                confirm_remove(target.clone(), taint, panel.clone(), window, cx)
            },
            window,
            cx,
        ),
    }
}

/// The URL Open Host opens: the host over https - or why not. The URL is
/// built, then read back, and its host must be the host asked for: anything
/// that would open somewhere else isn't opened.
pub(crate) fn host_url(host: &str) -> Result<String, String> {
    let refused = || format!("{host} isn't a host name that can be opened.");
    if !kind_actions::openable(host) {
        return Err(refused());
    }
    let url = url::Url::parse(&format!("https://{host}/")).map_err(|_| refused())?;
    if url.host_str() != Some(host) {
        return Err(refused());
    }
    Ok(url.to_string())
}

/// What a mutation asks, of what, and what it is called if it fails.
pub(super) struct Ask<T> {
    pub(super) question: Confirmation,
    pub(super) target: KindTarget,
    /// How a refusal names what was asked: "Suspend CronJob backup".
    pub(super) what: String,
    pub(super) panel: WeakEntity<T>,
}

/// Asks `ask`'s question, and on confirm sends `call` with the connection's
/// client. A refusal shows on the panel; success runs `done`.
pub(super) fn confirm_then<T, R, F>(
    ask: Ask<T>,
    call: impl Fn(kube::Client) -> F + 'static,
    done: impl Fn(R, &mut Window, &mut App) + Clone + 'static,
    window: &mut Window,
    cx: &mut App,
) where
    T: KindHost,
    R: Send + 'static,
    F: Future<Output = Result<R, ActionFailure>> + Send + 'static,
{
    let Ask {
        question,
        target,
        what,
        panel,
    } = ask;
    confirm_dialog::open(
        question,
        move |window, cx| {
            send(
                &target,
                what.clone(),
                panel.clone(),
                &call,
                done.clone(),
                window,
                cx,
            )
        },
        window,
        cx,
    );
}

/// Sends `call` with the current client - a refusal of its own if not
/// connected - and routes its result.
pub(super) fn send<T, R, F>(
    target: &KindTarget,
    what: String,
    panel: WeakEntity<T>,
    call: &impl Fn(kube::Client) -> F,
    done: impl Fn(R, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) where
    T: KindHost,
    R: Send + 'static,
    F: Future<Output = Result<R, ActionFailure>> + Send + 'static,
{
    let ConnectionState::Connected(client) = &target.connection.read(cx).state else {
        let failure = ActionFailure {
            message: format!("{} is not connected.", target.context_name),
            detail: String::new(),
        };
        return refuse(&panel, what, failure, cx);
    };
    let future = call(client.clone());
    let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let _ = tx.send(future.await).await;
    });
    let window_handle = window.window_handle();
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |result| {
            let _ = window_handle.update(cx, |_, window, cx| match result {
                Ok(value) => done(value, window, cx),
                Err(failure) => refuse(&panel, what.clone(), failure, cx),
            });
        })
        .await;
    })
    .detach();
}

fn refuse<T: KindHost>(
    panel: &WeakEntity<T>,
    action: String,
    failure: ActionFailure,
    cx: &mut App,
) {
    let _ = panel.update(cx, |this, cx| {
        this.show_refusal(Some(Refusal { action, failure }), cx)
    });
}

pub(super) fn question(
    title: String,
    body: ConfirmText,
    confirm: &str,
    severity: Severity,
) -> Confirmation {
    Confirmation {
        title: title.into(),
        body,
        confirm: confirm.to_string().into(),
        id_prefix: CONFIRM_ID_PREFIX,
        severity,
    }
}

/// `CronJob "backup" in "shop"`, the object a question names.
pub(super) fn named(target: &KindTarget) -> ConfirmText {
    let text = ConfirmText::new()
        .text(&format!("{} ", target.kind.gvk.kind))
        .name(&target.name);
    match &target.namespace {
        Some(namespace) => text.text(" in ").name(namespace),
        None => text,
    }
}

fn trigger_now<T: KindHost>(
    target: KindTarget,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let body = ConfirmText::new()
        .text("Create a Job from ")
        .append(named(&target))
        .text("'s job template now? Its schedule is unchanged.");
    let (namespace, name) = (target.namespace().to_string(), target.name.clone());
    let uid = target.facts.uid.clone();
    confirm_then(
        Ask {
            question: question(
                format!("Trigger {}?", target.noun()),
                body,
                "Trigger",
                Severity::Recoverable,
            ),
            target: target.clone(),
            what: format!("Trigger {}", target.noun()),
            panel,
        },
        move |client| {
            let (namespace, name, uid) = (namespace.clone(), name.clone(), uid.clone());
            async move { kind_actions::trigger_now(client, &namespace, &name, uid.as_deref()).await }
        },
        |job, window, cx| {
            use gpui_kit::component::WindowExt as _;
            use gpui_kit::component::notification::Notification;
            window.push_notification(Notification::success(format!("Created Job {job}")), cx);
        },
        window,
        cx,
    );
}

fn set_suspended<T: KindHost>(
    target: KindTarget,
    suspend: bool,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let verb = if suspend { "Suspend" } else { "Resume" };
    let effect = match (suspend, target.kind.gvk.kind.as_str()) {
        (true, "CronJob") => " It starts no new Jobs until resumed.",
        (true, _) => " Its running pods are stopped until it is resumed.",
        (false, _) => "",
    };
    let body = ConfirmText::new()
        .text(&format!("{verb} "))
        .append(named(&target))
        .text(&format!("?{effect}"));
    let (kind, namespace, name) = (
        target.kind.clone(),
        target.namespace().to_string(),
        target.name.clone(),
    );
    let uid = target.facts.uid.clone();
    confirm_then(
        Ask {
            question: question(
                format!("{verb} {}?", target.noun()),
                body,
                verb,
                Severity::Recoverable,
            ),
            target: target.clone(),
            what: format!("{verb} {}", target.noun()),
            panel,
        },
        move |client| {
            let (kind, namespace, name, uid) =
                (kind.clone(), namespace.clone(), name.clone(), uid.clone());
            async move {
                kind_actions::set_suspended(
                    client,
                    &kind,
                    &namespace,
                    &name,
                    suspend,
                    uid.as_deref(),
                )
                .await
            }
        },
        |(), _, _| {},
        window,
        cx,
    );
}

fn set_paused<T: KindHost>(
    target: KindTarget,
    pause: bool,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let verb = if pause {
        "Pause Rollout"
    } else {
        "Resume Rollout"
    };
    let body = ConfirmText::new()
        .text(if pause {
            "Pause the rollout of "
        } else {
            "Resume the rollout of "
        })
        .append(named(&target))
        .text(if pause {
            "? Changes to its pod template won't roll out until resumed."
        } else {
            "?"
        });
    let (kind, namespace, name) = (
        target.kind.clone(),
        target.namespace().to_string(),
        target.name.clone(),
    );
    let uid = target.facts.uid.clone();
    confirm_then(
        Ask {
            question: question(
                format!("{verb} for {}?", target.noun()),
                body,
                if pause { "Pause" } else { "Resume" },
                Severity::Recoverable,
            ),
            target: target.clone(),
            what: format!("{verb} for {}", target.noun()),
            panel,
        },
        move |client| {
            let (kind, namespace, name, uid) =
                (kind.clone(), namespace.clone(), name.clone(), uid.clone());
            async move {
                kind_actions::set_paused(client, &kind, &namespace, &name, pause, uid.as_deref())
                    .await
            }
        },
        |(), _, _| {},
        window,
        cx,
    );
}

#[cfg(test)]
mod tests;
