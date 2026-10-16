//! Running an entry (`exec-command-library` D3, D5): which pods it runs in -
//! the entry's own target from the library, or the resource it was run on -
//! then one pod runs at once, several ask in the chooser (first preselected),
//! and none is an error naming the entry and its scope. The container is the
//! entry's, else the pod's default, else its first; the session opens in the
//! main window's shell panel, running the entry's command (D1).
//!
//! Resolving takes a list request, so it runs off the UI thread; its answer
//! comes back to `window`, the window the user ran the entry from.

use super::chooser::{self, Choice};
use crate::config::exec_library::{Entry, Target, WorkloadKind};
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use crate::k8s::label_selector;
use crate::k8s::pod_resolver;
use crate::k8s::resource::exec::{ExecTarget, LibraryCommand};
use gpui_kit::*;
use k8s_openapi::api::core::v1::Pod;

/// Where an entry is run.
#[derive(Clone, Debug)]
pub(crate) enum Place {
    /// From the library: the entry's own target says where.
    Library,
    /// On this pod - Run from Library… on a pod.
    Pod(Box<Pod>),
    /// On this workload's pods - Run from Library… on a workload or Service.
    Workload {
        kind: WorkloadKind,
        namespace: String,
        name: String,
    },
}

/// The debug title of the pod chooser, for tests.
pub(crate) const POD_CHOOSER_TITLE: &str = "Run in Which Pod?";

/// Runs `entry` in `context_name`'s cluster at `place`. Errors are reported
/// in `window`; the session opens in the main window `window` holds.
pub(crate) fn run(
    entry: Entry,
    context_name: String,
    place: Place,
    window: &mut Window,
    cx: &mut App,
) {
    let argv = match entry.argv() {
        Ok(argv) => argv,
        Err(error) => return report(&entry, &error.to_string(), window, cx),
    };
    let connection = ClusterRegistry::connection(cx, &context_name);
    let ConnectionState::Connected(client) = connection.read(cx).state.clone() else {
        return report(
            &entry,
            &format!("{context_name} is not connected."),
            window,
            cx,
        );
    };
    let command = LibraryCommand {
        entry_name: entry.name.clone(),
        argv,
    };
    let target = entry.target.clone();
    let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let _ = tx.send(pods_at(client, &target, place).await).await;
    });
    let window_handle = window.window_handle();
    cx.spawn(async move |cx| {
        crate::runtime::drain(rx, |pods| {
            let _ = window_handle.update(cx, |_, window, cx| {
                resolved(&entry, &context_name, command.clone(), pods, window, cx)
            });
        })
        .await;
    })
    .detach();
}

/// The Running pods `target` means at `place`.
async fn pods_at(client: kube::Client, target: &Target, place: Place) -> Result<Vec<Pod>, String> {
    match place {
        Place::Library => match target {
            Target::Any => Err(
                "An any-pod entry runs from a pod: use Run from Library… on the pod.".to_string(),
            ),
            _ => pod_resolver::resolve_pods(client, target).await,
        },
        Place::Pod(pod) => Ok(pod_resolver::is_running(&pod)
            .then_some(*pod)
            .into_iter()
            .collect()),
        Place::Workload {
            kind,
            namespace,
            name,
        } => {
            let selector =
                pod_resolver::workload_selector(client.clone(), kind, &namespace, &name).await?;
            let mut pods = pod_resolver::running_pods(client, Some(&namespace), &selector).await?;
            // A selector entry run on a workload runs in the workload's pods it
            // also matches.
            if let Target::Selector { selector, .. } = target {
                let own = label_selector::parse(selector).map_err(|error| error.to_string())?;
                pods.retain(|pod| label_selector::matches(&own, pod.metadata.labels.as_ref()));
            }
            Ok(pods)
        }
    }
}

/// One pod runs; several ask; none - or a failed list - is reported.
fn resolved(
    entry: &Entry,
    context_name: &str,
    command: LibraryCommand,
    pods: Result<Vec<Pod>, String>,
    window: &mut Window,
    cx: &mut App,
) {
    let pods = match pods {
        Ok(pods) => pods,
        Err(error) => return report(entry, &error, window, cx),
    };
    match pods.as_slice() {
        [] => {
            let scope = entry.target.describe();
            report(
                entry,
                &format!("No Running pod matches ({scope})."),
                window,
                cx,
            );
        }
        [pod] => launch(entry, context_name, command, pod, window, cx),
        _ => {
            let choices = pods
                .iter()
                .map(|pod| Choice {
                    label: pod.metadata.name.clone().unwrap_or_default(),
                    detail: pod.metadata.namespace.clone(),
                })
                .collect();
            let (entry, context_name) = (entry.clone(), context_name.to_string());
            chooser::open(
                POD_CHOOSER_TITLE,
                choices,
                move |index, window, cx| {
                    launch(
                        &entry,
                        &context_name,
                        command.clone(),
                        &pods[index],
                        window,
                        cx,
                    )
                },
                window,
                cx,
            );
        }
    }
}

/// Opens the session in `pod`, in the entry's container.
fn launch(
    entry: &Entry,
    context_name: &str,
    command: LibraryCommand,
    pod: &Pod,
    window: &mut Window,
    cx: &mut App,
) {
    let container = match pod_resolver::container_for(pod, entry.container.as_deref()) {
        Ok(container) => container,
        Err(error) => return report(entry, &error, window, cx),
    };
    let target = ExecTarget {
        namespace: pod.metadata.namespace.clone().unwrap_or_default(),
        pod: pod.metadata.name.clone().unwrap_or_default(),
        container,
        command: Some(command),
    };
    crate::util::shell::open_exec(context_name.to_string(), target, window, cx);
}

/// Reports that `entry` couldn't run, and why, as a notification.
pub(crate) fn report(entry: &Entry, reason: &str, window: &mut Window, cx: &mut App) {
    notify(
        &format!("Couldn't run “{}”", entry.name),
        reason,
        window,
        cx,
    );
}

/// A notification titled `title` saying `reason`.
pub(crate) fn notify(title: &str, reason: &str, window: &mut Window, cx: &mut App) {
    use gpui_kit::component::WindowExt as _;
    use gpui_kit::component::notification::Notification;
    let title = title.to_string();
    #[cfg(test)]
    cx.default_global::<Reported>()
        .0
        .push((title.clone(), reason.to_string()));
    window.push_notification(Notification::error(reason.to_string()).title(title), cx);
}

/// Test-only record of every report: gpui-component keeps a window's
/// notifications private.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Reported(pub(crate) Vec<(String, String)>);

#[cfg(test)]
impl Global for Reported {}

#[cfg(test)]
mod tests;
