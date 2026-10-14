//! A bulk action from start to end: the value Scale, Label and Annotate ask for
//! ([`super::prompt`]), the confirmation ([`super::confirm`]), the batch sent off
//! the UI thread, and its outcome shown on the panel ([`super::outcome`]). Copy
//! Names, Copy YAML and - up to [`BULK_LOGS_WITHOUT_CONFIRM`] pods - View Logs
//! change nothing, so they act at once.

use super::confirm::confirmation;
use super::host::{BulkHost, BulkSelection};
use super::outcome::BatchOutcome;
use super::prompt::{self, Field, Prompt};
use crate::consts::{BULK_EVICTION_TIMEOUT, BULK_LOGS_WITHOUT_CONFIRM};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::resource::bulk::{
    self, BulkAction, MetadataMap, Mutation, ObjectOutcome, count_of_kinds,
};
use crate::k8s::resource::resource_actions::ActionFailure;
use crate::ui::confirm_dialog;
use gpui_kit::*;

/// Starts `action` on `this` panel's checked rows - nothing, if they aren't
/// offered it (a stale palette entry, say).
pub(super) fn start<T: BulkHost>(
    action: BulkAction,
    this: &mut T,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let Some(selection) = this.bulk_selection(cx) else {
        return;
    };
    if !selection.offered().contains(&action) {
        return;
    }
    let panel = cx.weak_entity();
    match action {
        BulkAction::CopyNames => {
            let names: Vec<&str> = selection
                .targets
                .iter()
                .map(|target| target.name.as_str())
                .collect();
            crate::ui::copy::copy_text(&names.join("\n"), cx);
        }
        BulkAction::CopyYaml => copy_yaml(selection, panel, cx),
        BulkAction::ViewLogs => view_logs(selection, window, cx),
        BulkAction::Scale => ask_replicas(selection, panel, window, cx),
        BulkAction::Label => ask_metadata(MetadataMap::Labels, selection, panel, window, cx),
        BulkAction::Annotate => {
            ask_metadata(MetadataMap::Annotations, selection, panel, window, cx)
        }
        BulkAction::RestartRollout => {
            let at = jiff::Timestamp::now();
            confirm_and_run(
                action,
                selection,
                Mutation::RestartRollout { at },
                panel,
                window,
                cx,
            )
        }
        BulkAction::RollbackRollout => confirm_and_run(
            action,
            selection,
            Mutation::RollbackRollout,
            panel,
            window,
            cx,
        ),
        BulkAction::Cordon => {
            confirm_and_run(action, selection, Mutation::Cordon, panel, window, cx)
        }
        BulkAction::Uncordon => {
            confirm_and_run(action, selection, Mutation::Uncordon, panel, window, cx)
        }
        BulkAction::Drain => {
            let drain = Mutation::Drain {
                eviction_timeout: BULK_EVICTION_TIMEOUT,
            };
            confirm_and_run(action, selection, drain, panel, window, cx)
        }
        BulkAction::Delete => {
            confirm_and_run(action, selection, Mutation::Delete, panel, window, cx)
        }
    }
}

/// Asks first, then sends `mutation` for every checked object.
fn confirm_and_run<T: BulkHost>(
    action: BulkAction,
    selection: BulkSelection,
    mutation: Mutation,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let question = confirmation(action, &selection.targets, Some(&mutation));
    confirm_dialog::open(
        question,
        move |_window, cx| {
            // A restart is stamped when it is confirmed, not when it was asked.
            let mutation = match &mutation {
                Mutation::RestartRollout { .. } => Mutation::RestartRollout {
                    at: jiff::Timestamp::now(),
                },
                other => other.clone(),
            };
            run(action, selection.clone(), mutation, panel.clone(), cx);
        },
        window,
        cx,
    );
}

/// Sends the batch off the UI thread, shows it running, then its outcome.
fn run<T: BulkHost>(
    action: BulkAction,
    selection: BulkSelection,
    mutation: Mutation,
    panel: WeakEntity<T>,
    cx: &mut App,
) {
    let ConnectionState::Connected(client) = &selection.connection.read(cx).state else {
        let failure = ActionFailure {
            message: format!("{} is not connected.", selection.context_name),
            detail: String::new(),
        };
        let outcomes: Vec<ObjectOutcome> = selection
            .targets
            .iter()
            .map(|target| ObjectOutcome {
                target: target.clone(),
                part: None,
                result: Err(failure.clone()),
            })
            .collect();
        finish(action, &panel, &outcomes, cx);
        return;
    };
    let client = client.clone();
    let total = selection.targets.len();
    let _ = panel.update(cx, |this, cx| {
        *this.bulk_outcome() = Some(BatchOutcome::running(action.label(), total));
        cx.notify();
    });
    let targets = selection.targets;
    let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let outcomes = bulk::run(client, &mutation, &targets).await;
        let _ = tx.send(outcomes).await;
    });
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |outcomes| {
            cx.update(|cx| finish(action, &panel, &outcomes, cx));
        })
        .await;
    })
    .detach();
}

/// Shows the batch's outcome. A delete also unchecks what it deleted; any
/// other action leaves the set checked, to act on again.
fn finish<T: BulkHost>(
    action: BulkAction,
    panel: &WeakEntity<T>,
    outcomes: &[ObjectOutcome],
    cx: &mut App,
) {
    let outcome = BatchOutcome::of(action.label(), outcomes);
    let _ = panel.update(cx, |this, cx| {
        if action == BulkAction::Delete {
            let succeeded = outcome.succeeded.clone();
            this.edit_checked(cx, |checked| checked.uncheck(&succeeded));
        }
        *this.bulk_outcome() = Some(outcome);
        cx.notify();
    });
}

/// Scale's prompt: one replica count, a whole number from 0 up.
fn ask_replicas<T: BulkHost>(
    selection: BulkSelection,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let counted = count_of_kinds(selection.targets.iter().map(|target| &target.kind));
    prompt::open(
        Prompt {
            title: format!("Scale {counted}").into(),
            fields: vec![Field {
                label: "Replicas",
                placeholder: "3",
            }],
            id_prefix: SCALE_PROMPT_ID,
        },
        |values| parse_replicas(&values[0]),
        move |replicas, window, cx| {
            let mutation = Mutation::Scale { replicas };
            confirm_and_run(
                BulkAction::Scale,
                selection.clone(),
                mutation,
                panel.clone(),
                window,
                cx,
            );
        },
        window,
        cx,
    );
}

/// The Scale prompt's id prefix.
pub(crate) const SCALE_PROMPT_ID: &str = "bulk-scale";
/// The Label and Annotate prompt's id prefix.
pub(crate) const METADATA_PROMPT_ID: &str = "bulk-metadata";

pub(super) fn parse_replicas(value: &str) -> Result<i32, String> {
    value
        .parse::<i32>()
        .ok()
        .filter(|replicas| *replicas >= 0)
        .ok_or_else(|| "Enter a whole number of replicas, 0 or more.".to_string())
}

/// Label's or Annotate's prompt: a key, and a value - empty removes the key.
fn ask_metadata<T: BulkHost>(
    map: MetadataMap,
    selection: BulkSelection,
    panel: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
) {
    let (action, title) = match map {
        MetadataMap::Labels => (BulkAction::Label, "Label"),
        MetadataMap::Annotations => (BulkAction::Annotate, "Annotate"),
    };
    let counted = count_of_kinds(selection.targets.iter().map(|target| &target.kind));
    prompt::open(
        Prompt {
            title: format!("{title} {counted}").into(),
            fields: vec![
                Field {
                    label: "Key",
                    placeholder: "team",
                },
                Field {
                    label: "Value (leave empty to remove the key)",
                    placeholder: "platform",
                },
            ],
            id_prefix: METADATA_PROMPT_ID,
        },
        |values| parse_metadata(&values[0], &values[1]),
        move |(key, value), window, cx| {
            let mutation = Mutation::SetMetadata { map, key, value };
            confirm_and_run(
                action,
                selection.clone(),
                mutation,
                panel.clone(),
                window,
                cx,
            );
        },
        window,
        cx,
    );
}

pub(super) fn parse_metadata(key: &str, value: &str) -> Result<(String, Option<String>), String> {
    if key.is_empty() {
        return Err("Enter a key.".to_string());
    }
    if key.contains(char::is_whitespace) || key.contains('=') {
        return Err("A key can't hold spaces or \"=\".".to_string());
    }
    let value = (!value.is_empty()).then(|| value.to_string());
    Ok((key.to_string(), value))
}

/// One Logs panel per checked pod - asking first past
/// [`BULK_LOGS_WITHOUT_CONFIRM`], so a large set doesn't flood the dock.
fn view_logs(selection: BulkSelection, window: &mut Window, cx: &mut App) {
    let pods = selection.pods.clone();
    if pods.len() <= BULK_LOGS_WITHOUT_CONFIRM {
        crate::util::shell::open_pods_logs(pods, window, cx);
        return;
    }
    let question = confirmation(BulkAction::ViewLogs, &selection.targets, None);
    confirm_dialog::open(
        question,
        move |window, cx| crate::util::shell::open_pods_logs(pods.clone(), window, cx),
        window,
        cx,
    );
}

/// Each checked object's manifest, fetched (rows keep only what they show),
/// redacted like the YAML view, joined by `---`. Any object that couldn't be
/// read is reported; the rest are copied.
fn copy_yaml<T: BulkHost>(selection: BulkSelection, panel: WeakEntity<T>, cx: &mut App) {
    let ConnectionState::Connected(client) = &selection.connection.read(cx).state else {
        return;
    };
    let client = client.clone();
    let targets = selection.targets;
    let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let _ = tx.send(bulk::manifests(client, &targets).await).await;
    });
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |(manifests, failures)| {
            cx.update(|cx| {
                crate::ui::copy::copy_text(&manifests.join("---\n"), cx);
                if !failures.is_empty() {
                    finish(BulkAction::CopyYaml, &panel, &failures, cx);
                }
            });
        })
        .await;
    })
    .detach();
}

#[cfg(test)]
mod tests;
