//! Running `helm` once (`helm-releases` D8): no shell, its own process group,
//! the login shell's PATH, output captured, and a timeout after which the
//! whole group is killed - a hook or exec plugin `helm` started included.
//!
//! [`RunSlots`] keeps it to one run per release, so a second Rollback while
//! the first is still going is refused instead of racing it.

use crate::consts::HELM_ERROR_LINES;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::process::Command;

/// One `helm` invocation, ready to run.
#[derive(Clone, Debug)]
pub(crate) struct HelmCommand {
    pub binary: PathBuf,
    /// The action's own arguments, e.g. `rollback web 3 -n shop`.
    pub args: Vec<String>,
    pub kubeconfig: PathBuf,
    pub context: String,
    /// The login shell's PATH, for `helm` and any exec plugin it runs.
    pub path_env: String,
    pub timeout: Duration,
}

/// What a successful run printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelmOutput {
    pub stdout: String,
    pub stderr: String,
}

/// Why a run failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HelmFailure {
    /// It couldn't be started, or set up to start.
    Start(String),
    /// It exited unsuccessfully: its code, and stderr's last lines.
    Exit { code: Option<i32>, message: String },
    /// It ran past its timeout and was stopped.
    TimedOut(Duration),
    /// Another run on the same release hasn't finished.
    AlreadyRunning,
}

impl fmt::Display for HelmFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(reason) => write!(f, "{reason}"),
            Self::Exit { code, message } => {
                match code {
                    Some(code) => write!(f, "helm exited with status {code}")?,
                    None => write!(f, "helm was stopped by a signal")?,
                }
                if message.is_empty() {
                    Ok(())
                } else {
                    write!(f, ": {message}")
                }
            }
            Self::TimedOut(after) => write!(
                f,
                "helm didn't finish within {} minutes and was stopped",
                after.as_secs() / 60
            ),
            Self::AlreadyRunning => write!(f, "A helm action on this release is already running"),
        }
    }
}

/// Runs `command`, killing its process group if it outlasts its timeout.
pub(crate) async fn run(command: HelmCommand) -> Result<HelmOutput, HelmFailure> {
    let mut process = Command::new(&command.binary);
    process
        .args(&command.args)
        .arg("--kubeconfig")
        .arg(&command.kubeconfig)
        .arg("--kube-context")
        .arg(&command.context)
        .env("PATH", &command.path_env)
        // The flags win over these anyway; set them so nothing helm runs
        // (a plugin, a hook's kubectl) finds the user's own.
        .env("KUBECONFIG", &command.kubeconfig)
        .env_remove("HELM_KUBECONTEXT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    process.process_group(0);
    #[cfg(not(test))]
    let spawned = process.spawn();
    // Parallel tests can briefly hold a fake `helm` open (`test_helm::past_busy`).
    #[cfg(test)]
    let spawned = super::test_helm::past_busy(|| std::future::ready(process.spawn())).await;
    let child = spawned.map_err(|error| {
        HelmFailure::Start(format!(
            "Couldn't run {}: {error}",
            command.binary.display()
        ))
    })?;
    // Armed until `helm` has been waited for: a timeout, or the run being
    // dropped (the app quitting mid-run), kills the whole group - hooks and
    // plugins `helm` started too, which `kill_on_drop` alone would leave.
    let mut group = GroupKill(child.id());
    let waited = tokio::time::timeout(command.timeout, child.wait_with_output()).await;
    if waited.is_ok() {
        group.disarm();
    }
    match waited {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            if output.status.success() {
                Ok(HelmOutput { stdout, stderr })
            } else {
                Err(HelmFailure::Exit {
                    code: output.status.code(),
                    message: last_lines(&stderr, HELM_ERROR_LINES),
                })
            }
        }
        Ok(Err(error)) => Err(HelmFailure::Start(format!("helm failed: {error}"))),
        // `group` drops on the way out, killing the group.
        Err(_) => Err(HelmFailure::TimedOut(command.timeout)),
    }
}

/// Kills `helm`'s process group when dropped, unless disarmed once `helm`
/// has been waited for - after which its pid may belong to someone else.
struct GroupKill(Option<u32>);

impl GroupKill {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for GroupKill {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            crate::util::pidfile::kill_process_group(pid);
        }
    }
}

/// `text`'s last `count` non-blank lines, joined - where `helm` says what
/// went wrong.
fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// The release a run acts on: what [`RunSlots`] allows one run at a time for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ReleaseKey {
    pub context: String,
    pub namespace: String,
    pub release: String,
}

/// The releases with a run going.
#[derive(Clone, Default)]
pub(crate) struct RunSlots(Arc<Mutex<HashSet<ReleaseKey>>>);

/// The app's one set of running releases.
pub(super) static RUNS: LazyLock<RunSlots> = LazyLock::new(RunSlots::default);

impl RunSlots {
    /// Claims `key` for a run, or refuses while another run holds it. The
    /// claim ends when the returned slot drops.
    pub(crate) fn claim(&self, key: ReleaseKey) -> Result<RunSlot, HelmFailure> {
        if !self.0.lock().insert(key.clone()) {
            return Err(HelmFailure::AlreadyRunning);
        }
        Ok(RunSlot {
            slots: self.0.clone(),
            key,
        })
    }
}

/// One release's claim on a run, released when dropped.
pub(crate) struct RunSlot {
    slots: Arc<Mutex<HashSet<ReleaseKey>>>,
    key: ReleaseKey,
}

impl Drop for RunSlot {
    fn drop(&mut self) {
        self.slots.lock().remove(&self.key);
    }
}

#[cfg(test)]
mod tests;
