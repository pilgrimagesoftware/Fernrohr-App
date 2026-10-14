//! What each bulk action's confirmation asks: the action, how many of which
//! kind(s) ("Restart Rollout for 2 Deployments, 1 DaemonSet?"), any value being
//! set, and the objects by name.
//!
//! Delete and Drain are irreversible - the dialog opens on Cancel and confirms
//! only by a deliberate press (`keyboard-first.md`); the rest are recoverable,
//! since the same action, or its opposite, puts them back.

use crate::k8s::resource::bulk::{BulkAction, BulkTarget, MetadataMap, Mutation, count_of_kinds};
use crate::ui::confirm_dialog::{Confirmation, Severity};
use crate::ui::confirm_text::ConfirmText;

/// Every bulk confirmation's id prefix: its buttons are `bulk-confirm-cancel`
/// and `bulk-confirm-confirm`.
pub(crate) const CONFIRM_ID_PREFIX: &str = "bulk-confirm";

/// How many names a confirmation lists before "and N more".
const LISTED_NAMES: usize = 8;

/// The question for `action` on `targets`, `mutation` carrying what Scale,
/// Label and Annotate set.
pub(crate) fn confirmation(
    action: BulkAction,
    targets: &[BulkTarget],
    mutation: Option<&Mutation>,
) -> Confirmation {
    let counted = count_of_kinds(targets.iter().map(|target| &target.kind));
    let (title, confirm) = match (action, mutation) {
        (BulkAction::Scale, Some(Mutation::Scale { replicas })) => (
            format!(
                "Scale {counted} to {replicas} {}?",
                replicas_word(*replicas)
            ),
            "Scale".to_string(),
        ),
        (
            BulkAction::Label | BulkAction::Annotate,
            Some(Mutation::SetMetadata { map, key, value }),
        ) => {
            let what = match map {
                MetadataMap::Labels => "label",
                MetadataMap::Annotations => "annotation",
            };
            match value {
                Some(value) => (
                    format!("Set {what} {key}={value} on {counted}?"),
                    "Set".to_string(),
                ),
                None => (
                    format!("Remove {what} {key} from {counted}?"),
                    "Remove".to_string(),
                ),
            }
        }
        (BulkAction::RestartRollout | BulkAction::RollbackRollout, _) => (
            format!("{} for {counted}?", action.label()),
            action.label().to_string(),
        ),
        (BulkAction::ViewLogs, _) => (
            format!("Open {} Logs panels?", targets.len()),
            "Open".to_string(),
        ),
        _ => (
            format!("{} {counted}?", action.label()),
            action.label().to_string(),
        ),
    };
    Confirmation {
        title: title.into(),
        body: body(action, targets),
        confirm: confirm.into(),
        id_prefix: CONFIRM_ID_PREFIX,
        severity: severity(action),
    }
}

pub(crate) fn severity(action: BulkAction) -> Severity {
    match action {
        BulkAction::Delete | BulkAction::Drain => Severity::Irreversible,
        _ => Severity::Recoverable,
    }
}

fn replicas_word(replicas: i32) -> &'static str {
    if replicas == 1 { "replica" } else { "replicas" }
}

/// The objects by name, and what the action does beyond the obvious.
fn body(action: BulkAction, targets: &[BulkTarget]) -> ConfirmText {
    let names: Vec<String> = targets
        .iter()
        .take(LISTED_NAMES)
        .map(BulkTarget::display_name)
        .collect();
    let mut text = ConfirmText::new().names(&names);
    if targets.len() > LISTED_NAMES {
        text = text.text(&format!(" and {} more", targets.len() - LISTED_NAMES));
    }
    let text = text.text(".");
    match action {
        BulkAction::Delete if targets.iter().any(|target| target.kind.is_core_pod()) => {
            text.text(" A controller that owns a pod may start a replacement.")
        }
        BulkAction::Drain => text.text(
            " Each is cordoned, then its pods are evicted, all but DaemonSet and mirror pods. \
             A PodDisruptionBudget can refuse an eviction.",
        ),
        BulkAction::RollbackRollout => {
            text.text(" Each goes back to the revision before its current one.")
        }
        BulkAction::Cordon => text.text(" No new pods are scheduled on them."),
        _ => text,
    }
}

#[cfg(test)]
mod tests;
