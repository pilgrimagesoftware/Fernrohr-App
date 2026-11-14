//! Finding `helm` (`helm-releases` D8): the path set in Settings, else `helm`
//! on the login shell's PATH, accepted only when `helm version --short`
//! reports Helm 3 or 4.
//!
//! The answer is checked off the UI thread and cached in [`HelmLocation`],
//! refreshed when the Settings path changes, so anything deciding whether a
//! Helm action is available reads [`status`] and never runs a process on a
//! render path.

use crate::consts::HELM_VERSION_TIMEOUT;
use crate::util::which::{find_program, is_executable};
use gpui_kit::{App, Global, Task};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// A usable `helm`: where it is and the version it reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HelmBinary {
    pub path: PathBuf,
    /// `helm version --short`'s answer, e.g. `v3.14.2+gc309b6f`.
    pub version: String,
}

/// What Helm actions can run with right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HelmStatus {
    /// Not checked yet, or a check is running.
    Checking,
    Ready(HelmBinary),
    /// No usable `helm`, and why - shown wherever an action is disabled.
    Unusable(String),
}

/// The cached answer, and the check that will replace it. A newer check
/// replaces the task, which drops - and so cancels - an older one, so a slow
/// stale check can never overwrite a fresh answer.
struct HelmLocation {
    status: HelmStatus,
    _check: Option<Task<()>>,
    /// The path the last check was asked to look at - what tests assert in
    /// place of running one.
    #[cfg(test)]
    requested: Option<PathBuf>,
}

impl Global for HelmLocation {}

/// Where Helm actions stand: the last check's answer, or
/// [`HelmStatus::Checking`] before one has finished.
pub(crate) fn status(cx: &App) -> HelmStatus {
    cx.try_global::<HelmLocation>()
        .map_or(HelmStatus::Checking, |location| location.status.clone())
}

/// Checks for `helm` again - at `configured` when set, else on the login
/// shell's PATH - off the UI thread, and caches the answer. Windows redraw
/// once it's in, so anything showing it catches up.
#[cfg(not(test))]
pub(super) fn refresh(configured: Option<PathBuf>, cx: &mut App) {
    let checked = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        // `login_path` blocks on the login shell the first time it runs.
        let path_env = tokio::task::spawn_blocking(crate::util::login_env::login_path)
            .await
            .unwrap_or_default();
        let _ = tx.send(locate(configured.as_deref(), path_env).await).await;
    });
    let check = cx.spawn(async move |cx: &mut gpui_kit::AsyncApp| {
        let mut checked = checked;
        let Some(found) = checked.recv().await else {
            return;
        };
        let status = match found {
            Ok(binary) => HelmStatus::Ready(binary),
            Err(reason) => HelmStatus::Unusable(reason),
        };
        cx.update(|cx| {
            if cx.has_global::<HelmLocation>() {
                cx.global_mut::<HelmLocation>().status = status;
            }
            cx.refresh_windows();
        });
    });
    cx.set_global(HelmLocation {
        status: HelmStatus::Checking,
        _check: Some(check),
    });
}

/// Tests check `locate` directly: a check here would run the login shell and
/// wake the test executor from a tokio thread. This records what was asked.
#[cfg(test)]
pub(super) fn refresh(configured: Option<PathBuf>, cx: &mut App) {
    cx.set_global(HelmLocation {
        status: HelmStatus::Checking,
        _check: None,
        requested: configured,
    });
}

/// Test-only: the path the last check was asked to look at.
#[cfg(test)]
pub(crate) fn requested(cx: &App) -> Option<PathBuf> {
    cx.try_global::<HelmLocation>()?.requested.clone()
}

/// Test-only: makes `status` the cached answer, as a finished check would.
#[cfg(test)]
pub(crate) fn set_status(status: HelmStatus, cx: &mut App) {
    cx.set_global(HelmLocation {
        status,
        _check: None,
        requested: None,
    });
}

/// The `helm` at `configured`, or else on `path_env`, if it is Helm 3 or 4 - else
/// why not, in words for the person who must fix it.
pub(super) async fn locate(
    configured: Option<&Path>,
    path_env: &str,
) -> Result<HelmBinary, String> {
    let path = match configured {
        Some(path) if is_executable(path) => path.to_path_buf(),
        Some(path) => return Err(format!("There's no helm program at {}.", path.display())),
        None => find_program("helm", path_env).ok_or(
            "helm isn't on your login shell's PATH. Install Helm 3 or 4, or set its path in \
             Settings.",
        )?,
    };
    let mut version_command = Command::new(&path);
    version_command
        .args(["version", "--short"])
        .env("PATH", path_env)
        .kill_on_drop(true);
    #[cfg(not(test))]
    let ran = version_command.output();
    // Parallel tests can briefly hold a fake `helm` open (`test_helm::past_busy`).
    #[cfg(test)]
    let ran = super::test_helm::past_busy(|| version_command.output());
    let output = tokio::time::timeout(HELM_VERSION_TIMEOUT, ran)
        .await
        .map_err(|_| format!("{} didn't report its version in time.", path.display()))?
        .map_err(|error| format!("Couldn't run {}: {error}.", path.display()))?;
    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if !output.status.success() && version.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{} version failed: {}",
            path.display(),
            stderr.trim()
        ));
    }
    version_is_supported(&version)
        .then(|| HelmBinary {
            path: path.clone(),
            version: version.clone(),
        })
        .ok_or_else(|| {
            format!(
                "{} is Helm {}; Helm 3 or 4 is required.",
                path.display(),
                if version.is_empty() {
                    "of unknown version"
                } else {
                    &version
                }
            )
        })
}

/// Whether `helm version --short`'s first line reports Helm 3 or 4 (meta
/// #264): their `rollback` and `uninstall` are the same. Helm 2 prints
/// `Client: v2.17.0+g…`, and a later major isn't trusted until checked, so
/// only a line starting `v3.` or `v4.` passes.
fn version_is_supported(version: &str) -> bool {
    ["v3.", "v4."]
        .iter()
        .any(|major| version.starts_with(major))
}

#[cfg(test)]
mod tests;
