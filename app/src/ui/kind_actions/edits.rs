//! The kind actions that ask for something before they confirm: Expand's new
//! size, Add Taint's key, value and effect, and the taint Remove Taint removes.

use super::flow::{Ask, confirm_then, named, question};
use super::host::{KindHost, KindTarget};
use super::taints;
use crate::k8s::resource::kind_actions::{self, Taint};
use crate::ui::bulk::prompt::{self, Field, Prompt};
use crate::ui::confirm_dialog::Severity;
use crate::ui::confirm_text::ConfirmText;
use gpui_kit::*;

/// The prompts' id prefixes.
pub(crate) const EXPAND_PROMPT_ID: &str = "kind-expand";
pub(crate) const TAINT_PROMPT_ID: &str = "kind-taint";

/// Expand's prompt: a size larger than the claim's - Kubernetes never shrinks
/// one - then a confirmation naming both sizes, irreversible for that reason.
pub(super) fn ask_size<T: KindHost>(
    target: KindTarget,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let current = target.facts.storage.clone().unwrap_or_default();
    prompt::open(
        Prompt {
            title: format!("Expand {}", target.noun()).into(),
            fields: vec![Field {
                label: "New size",
                placeholder: "20Gi",
            }],
            id_prefix: EXPAND_PROMPT_ID,
        },
        {
            let current = current.clone();
            move |values| parse_size(&values[0], &current)
        },
        move |size: String, window, cx| {
            let body = ConfirmText::new()
                .text("Expand ")
                .append(named(&target))
                .text(" from ")
                .name(&current)
                .text(" to ")
                .name(&size)
                .text("? A claim can't be made smaller again.");
            let (kind, namespace, name) = (
                target.kind.clone(),
                target.namespace().to_string(),
                target.name.clone(),
            );
            let uid = target.facts.uid.clone();
            let call_size = size.clone();
            confirm_then(
                Ask {
                    question: question(
                        format!("Expand {}?", target.noun()),
                        body,
                        "Expand",
                        Severity::Irreversible,
                    ),
                    target: target.clone(),
                    what: format!("Expand {} to {size}", target.noun()),
                    panel: panel.clone(),
                },
                move |client| {
                    let (kind, namespace, name, size, uid) = (
                        kind.clone(),
                        namespace.clone(),
                        name.clone(),
                        call_size.clone(),
                        uid.clone(),
                    );
                    async move {
                        kind_actions::expand(
                            client,
                            &kind,
                            &namespace,
                            &name,
                            &size,
                            uid.as_deref(),
                        )
                        .await
                    }
                },
                |(), _, _| {},
                window,
                cx,
            );
        },
        window,
        cx,
    );
}

/// A new size, larger than `current`.
pub(super) fn parse_size(size: &str, current: &str) -> Result<String, String> {
    let Some(new) = kind_actions::bytes(size) else {
        return Err("Enter a size such as 20Gi or 500M.".to_string());
    };
    match kind_actions::bytes(current) {
        Some(old) if new <= old => Err(format!(
            "Enter a size larger than the claim's {current}: a claim can't shrink."
        )),
        _ => Ok(size.to_string()),
    }
}

pub(super) fn ask_taint<T: KindHost>(
    target: KindTarget,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let existing = target.facts.taints.clone();
    prompt::open(
        Prompt {
            title: format!("Add a Taint to {}", target.noun()).into(),
            fields: vec![
                Field {
                    label: "Key",
                    placeholder: "dedicated",
                },
                Field {
                    label: "Value (optional)",
                    placeholder: "gpu",
                },
                Field {
                    label: "Effect: NoSchedule, PreferNoSchedule or NoExecute",
                    placeholder: "NoSchedule",
                },
            ],
            id_prefix: TAINT_PROMPT_ID,
        },
        move |values| taints::parse(&values[0], &values[1], &values[2], &existing),
        move |taint: Taint, window, cx| {
            let warning = if taint.effect == "NoExecute" {
                " Pods on it that don't tolerate it are evicted."
            } else {
                ""
            };
            let body = ConfirmText::new()
                .text("Add taint ")
                .name(&taint.label())
                .text(" to ")
                .append(named(&target))
                .text(&format!("?{warning}"));
            let (kind, name, facts) = (
                target.kind.clone(),
                target.name.clone(),
                target.facts.clone(),
            );
            let added = taint.clone();
            confirm_then(
                Ask {
                    question: question(
                        format!("Add a Taint to {}?", target.noun()),
                        body,
                        "Add",
                        Severity::Recoverable,
                    ),
                    target: target.clone(),
                    what: format!("Add taint {} to {}", taint.label(), target.noun()),
                    panel: panel.clone(),
                },
                move |client| {
                    let (kind, name, facts, taint) =
                        (kind.clone(), name.clone(), facts.clone(), added.clone());
                    async move { kind_actions::add_taint(client, &kind, &name, &facts, &taint).await }
                },
                |(), _, _| {},
                window,
                cx,
            );
        },
        window,
        cx,
    );
}

pub(super) fn confirm_remove<T: KindHost>(
    target: KindTarget,
    taint: Taint,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let body = ConfirmText::new()
        .text("Remove taint ")
        .name(&taint.label())
        .text(" from ")
        .append(named(&target))
        .text("?");
    let (kind, name, facts) = (
        target.kind.clone(),
        target.name.clone(),
        target.facts.clone(),
    );
    let removed = taint.clone();
    confirm_then(
        Ask {
            question: question(
                format!("Remove a Taint from {}?", target.noun()),
                body,
                "Remove",
                Severity::Recoverable,
            ),
            target: target.clone(),
            what: format!("Remove taint {} from {}", taint.label(), target.noun()),
            panel,
        },
        move |client| {
            let (kind, name, facts, taint) =
                (kind.clone(), name.clone(), facts.clone(), removed.clone());
            async move { kind_actions::remove_taint(client, &kind, &name, &facts, &taint).await }
        },
        |(), _, _| {},
        window,
        cx,
    );
}
