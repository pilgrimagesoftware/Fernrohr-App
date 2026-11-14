//! An action from request to result (`helm-release-actions`): refused with
//! the reason when `helm` or the connection isn't available or an action is
//! already running; else confirmed - Rollback recoverably, Uninstall
//! irreversibly - then run through `helm_cli` while the panel says so, and a
//! failure left on the panel with `helm`'s own message. Success needs no
//! message: the release watch shows the new revision, or the release gone.

use super::manifest::fetch_count;
use crate::helm_cli::{self, HelmAction, HelmFailure, ReleaseKey};
use crate::k8s::resource::delete_flow::refusal::Refusal;
use crate::k8s::resource::resource_actions::ActionFailure;
use crate::ui::confirm_dialog::{self, Confirmation, Detail, Severity};
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::*;

/// The confirmations' element-id prefixes.
pub(crate) const ROLLBACK_DIALOG: &str = "helm-rollback";
pub(crate) const UNINSTALL_DIALOG: &str = "helm-uninstall";

/// A panel's actions: the one running, and the last refusal.
#[derive(Clone, Debug, Default)]
pub(crate) struct ActionState {
    /// What is running, as the panel says it: "Rolling back web to revision 3".
    pub(crate) running: Option<String>,
    /// The last action refused or failed, until dismissed or another starts.
    pub(crate) refusal: Option<Refusal>,
}

/// A panel that offers release actions.
pub(crate) trait ActionHost: Sized + 'static {
    fn action_state(&mut self) -> &mut ActionState;
}

/// One revision, as a confirmation names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RevisionLine {
    pub(crate) version: u32,
    /// `web-1.2.0`, when the revision has been read.
    pub(crate) chart: Option<String>,
    pub(crate) app_version: Option<String>,
}

impl RevisionLine {
    /// "revision 3 · web-1.2.0 · app 2.0".
    pub(crate) fn describe(&self) -> String {
        let mut text = format!("revision {}", self.version);
        if let Some(chart) = &self.chart {
            text.push_str(&format!(" \u{00b7} {chart}"));
        }
        if let Some(app) = &self.app_version {
            text.push_str(&format!(" \u{00b7} app {app}"));
        }
        text
    }
}

/// A rollback to ask about: the release, where it is, where it would go.
#[derive(Clone, Debug)]
pub(crate) struct Rollback {
    pub(crate) release: ReleaseKey,
    pub(crate) current: RevisionLine,
    pub(crate) target: RevisionLine,
}

/// Leaves `message` on the panel as why `what` didn't happen.
pub(crate) fn refuse<T: ActionHost>(
    this: &mut T,
    what: &str,
    message: String,
    cx: &mut Context<T>,
) {
    this.action_state().refusal = Some(Refusal {
        action: what.to_string(),
        failure: ActionFailure {
            message,
            detail: String::new(),
        },
    });
    cx.notify();
}

/// Whether `what` on `release` may be asked about now; when not, the panel
/// says why - except while an action runs, which the panel already shows:
/// a refusal then would only surface, stale, once the run had ended.
fn ready<T: ActionHost>(
    this: &mut T,
    what: &str,
    release: &ReleaseKey,
    cx: &mut Context<T>,
) -> bool {
    if this.action_state().running.is_some() {
        return false;
    }
    if let Err(why) = helm_cli::availability(&release.context, cx) {
        refuse(this, what, why.to_string(), cx);
        return false;
    }
    true
}

/// Asks to roll `rollback.release` back - recoverably, so Enter may confirm -
/// and runs it if confirmed.
pub(crate) fn confirm_rollback<T: ActionHost>(
    this: &mut T,
    rollback: Rollback,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let name = rollback.release.release.clone();
    let what = format!("Roll back {name}");
    if !ready(this, &what, &rollback.release, cx) {
        return;
    }
    let (from, to) = (rollback.current.version, rollback.target.version);
    let confirmation = Confirmation {
        title: format!("Roll back {name}?").into(),
        body: ConfirmText::new()
            .text("Roll ")
            .name(&name)
            .text(&format!(" back from revision {from} to revision {to}?")),
        confirm: "Roll back".into(),
        id_prefix: ROLLBACK_DIALOG,
        severity: Severity::Recoverable,
    };
    let details = vec![
        detail("Release", &name),
        detail("Namespace", &rollback.release.namespace),
        detail("Current", &rollback.current.describe()),
        detail("Roll back to", &rollback.target.describe()),
    ];
    let weak = cx.weak_entity();
    let release = rollback.release;
    confirm_dialog::open_with(
        confirmation,
        details,
        move |_window, cx| {
            run(
                weak.clone(),
                release.clone(),
                HelmAction::Rollback { revision: to },
                format!("Rolling back {name} to revision {to}"),
                what.clone(),
                cx,
            )
        },
        |_, _| {},
        window,
        cx,
    );
}

/// Asks to uninstall `release` - irreversibly, so focus starts on Cancel and
/// Enter cancels - and runs it if confirmed. `deployed` is the client and
/// revision Secret to count the deployed manifest's objects from; the
/// confirmation opens once they're counted.
pub(crate) fn confirm_uninstall<T: ActionHost>(
    this: &mut T,
    release: ReleaseKey,
    deployed: Option<(kube::Client, String)>,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let what = format!("Uninstall {}", release.release);
    if !ready(this, &what, &release, cx) {
        return;
    }
    let counted = match deployed {
        Some((client, secret)) => fetch_count(client, release.namespace.clone(), secret, cx),
        None => Task::ready(None),
    };
    cx.spawn_in(window, async move |this, cx| {
        let count = counted.await;
        let _ = this.update_in(cx, |_this, window, cx| {
            open_uninstall(release, what, count, window, cx)
        });
    })
    .detach();
}

fn open_uninstall<T: ActionHost>(
    release: ReleaseKey,
    what: String,
    count: Option<usize>,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let name = release.release.clone();
    let objects = match count {
        Some(1) => "the 1 object".to_string(),
        Some(count) => format!("the {count} objects"),
        None => "the objects".to_string(),
    };
    let confirmation = Confirmation {
        title: format!("Uninstall {name}?").into(),
        body: ConfirmText::new()
            .text("Uninstalling ")
            .name(&name)
            .text(" from ")
            .name(&release.namespace)
            .text(&format!(
                " deletes {objects} its deployed manifest declares, and its release \
                 history. This can't be undone."
            )),
        confirm: "Uninstall".into(),
        id_prefix: UNINSTALL_DIALOG,
        severity: Severity::Irreversible,
    };
    let weak = cx.weak_entity();
    confirm_dialog::open(
        confirmation,
        move |_window, cx| {
            run(
                weak.clone(),
                release.clone(),
                HelmAction::Uninstall,
                format!("Uninstalling {name}"),
                what.clone(),
                cx,
            )
        },
        window,
        cx,
    );
}

fn detail(label: &'static str, value: &str) -> Detail {
    Detail {
        label: label.into(),
        value: value.to_string().into(),
    }
}

/// Runs `action` on `release`, saying `running` on the panel meanwhile and
/// leaving a failure as a refusal of `what`.
fn run<T: ActionHost>(
    panel: WeakEntity<T>,
    release: ReleaseKey,
    action: HelmAction,
    running: String,
    what: String,
    cx: &mut App,
) {
    let finished = helm_cli::start(release, action, cx);
    let _ = panel.update(cx, |this, cx| {
        let state = this.action_state();
        state.running = Some(running);
        state.refusal = None;
        cx.notify();
    });
    cx.spawn(async move |cx| {
        let result = finished.await;
        let _ = panel.update(cx, |this, cx| {
            let state = this.action_state();
            state.running = None;
            if let Err(failure) = result {
                state.refusal = Some(refusal_of(&what, failure));
            }
            cx.notify();
        });
    })
    .detach();
}

/// `failure` as the refusal of `what`: `helm`'s exit status as the message
/// and its own error lines as the detail.
pub(super) fn refusal_of(what: &str, failure: HelmFailure) -> Refusal {
    let failure = match failure {
        HelmFailure::Exit { code, message } => ActionFailure {
            message: match code {
                Some(code) => format!("helm exited with status {code}"),
                None => "helm was stopped by a signal".to_string(),
            },
            detail: message,
        },
        other => ActionFailure {
            message: other.to_string(),
            detail: String::new(),
        },
    };
    Refusal {
        action: what.to_string(),
        failure,
    }
}
