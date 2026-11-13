//! One Helm action's run, start to finish (`helm-releases` D8): whether it
//! can run at all ([`availability`]), then - off the UI thread - the context's
//! kubeconfig read, the run's own written, `helm` run against it and the file
//! removed. The connection, and with it any tunnel, is held for the run's
//! length so it can't be torn down underneath `helm`.

use super::kubeconfig::{self, Endpoint, RunKubeconfig};
use super::locate::{HelmBinary, HelmStatus, status};
use super::run::{HelmCommand, HelmFailure, HelmOutput, RUNS, ReleaseKey, RunSlots, run};
use crate::consts::HELM_ACTION_TIMEOUT;
use crate::k8s::cluster::connection::ConnectionState;
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::{App, AsyncApp, Task};
use kube::config::Kubeconfig;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Why a Helm action can't run now - what its disabled menu item, button and
/// palette entry say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HelmUnavailable {
    /// `helm` hasn't been checked for yet.
    Checking,
    /// No usable `helm`, and why.
    NoHelm(String),
    /// The context isn't connected.
    NotConnected,
}

impl fmt::Display for HelmUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Checking => write!(f, "Looking for helm…"),
            Self::NoHelm(reason) => write!(f, "{reason}"),
            Self::NotConnected => write!(f, "The context isn't connected."),
        }
    }
}

/// The `helm` an action on `context` would run, or why it can't run.
// UNWIRED(#370): section 8's Rollback and Uninstall read this for their
// disabled states. Only tests call it until then.
#[allow(dead_code)]
pub(crate) fn availability(context: &str, cx: &App) -> Result<HelmBinary, HelmUnavailable> {
    let binary = match status(cx) {
        HelmStatus::Ready(binary) => binary,
        HelmStatus::Checking => return Err(HelmUnavailable::Checking),
        HelmStatus::Unusable(reason) => return Err(HelmUnavailable::NoHelm(reason)),
    };
    let connected = ClusterRegistry::existing_connection(cx, context).is_some_and(|connection| {
        matches!(connection.read(cx).state, ConnectionState::Connected(_))
    });
    if connected {
        Ok(binary)
    } else {
        Err(HelmUnavailable::NotConnected)
    }
}

/// Runs `helm <args>` against `release`'s context and namespace, resolving
/// with what it printed or why it failed. Refused at once when `helm` or the
/// connection isn't available, or another run on the release is going.
// UNWIRED(#370): section 8's Rollback and Uninstall run through this. Only
// tests call it until then.
#[allow(dead_code)]
pub(crate) fn start(
    release: ReleaseKey,
    args: Vec<String>,
    cx: &mut App,
) -> Task<Result<HelmOutput, HelmFailure>> {
    let binary = match availability(&release.context, cx) {
        Ok(binary) => binary,
        Err(why) => return Task::ready(Err(HelmFailure::Start(why.to_string()))),
    };
    let Some(connection) = ClusterRegistry::existing_connection(cx, &release.context) else {
        return Task::ready(Err(HelmFailure::Start(
            HelmUnavailable::NotConnected.to_string(),
        )));
    };
    let endpoint = Endpoint::of(connection.read(cx).tunnel_route());
    let finished = start_on(&RUNS, release, args, binary, endpoint, cx);
    cx.spawn(async move |_: &mut AsyncApp| {
        // Holding the connection holds its tunnel: dropping its last handle
        // would stop the forward with `helm` still using it.
        let _connection = connection;
        finished.await
    })
}

/// [`start`]'s run once it may go: `release` claimed in `slots`, then the
/// rest off the UI thread.
pub(super) fn start_on(
    slots: &RunSlots,
    release: ReleaseKey,
    args: Vec<String>,
    binary: HelmBinary,
    endpoint: Endpoint,
    cx: &App,
) -> Task<Result<HelmOutput, HelmFailure>> {
    let slot = match slots.claim(release.clone()) {
        Ok(slot) => slot,
        Err(refused) => return Task::ready(Err(refused)),
    };
    let finished = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
        let _slot = slot;
        // Both block: the login shell the first time, and the file read.
        let path_env = tokio::task::spawn_blocking(crate::util::login_env::login_path)
            .await
            .unwrap_or_default();
        let source = tokio::task::spawn_blocking(Kubeconfig::read).await;
        let result = match source {
            Ok(Ok(source)) => {
                let run = RunSpec {
                    binary: binary.path,
                    args,
                    context: release.context,
                    endpoint,
                    dir: kubeconfig::dir(),
                    path_env: path_env.to_string(),
                    timeout: HELM_ACTION_TIMEOUT,
                };
                execute(run, &source).await
            }
            Ok(Err(error)) => Err(HelmFailure::Start(format!(
                "Couldn't read the kubeconfig: {error}"
            ))),
            Err(error) => Err(HelmFailure::Start(format!(
                "Couldn't read the kubeconfig: {error}"
            ))),
        };
        let _ = tx.send(result).await;
    });
    cx.background_executor().spawn(async move {
        let mut finished = finished;
        finished
            .recv()
            .await
            .unwrap_or_else(|| Err(HelmFailure::Start("The helm run was cancelled.".into())))
    })
}

/// Everything one run needs once the binary and endpoint are known.
pub(super) struct RunSpec {
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub context: String,
    pub endpoint: Endpoint,
    /// Where its kubeconfig is written.
    pub dir: PathBuf,
    /// The login shell's PATH, for `helm` and any exec plugin it runs.
    pub path_env: String,
    pub timeout: Duration,
}

/// Writes the run's kubeconfig from `source`, runs `helm` against it, and
/// removes it, whatever the outcome.
pub(super) async fn execute(spec: RunSpec, source: &Kubeconfig) -> Result<HelmOutput, HelmFailure> {
    let config =
        kubeconfig::for_run(source, &spec.context, spec.endpoint).map_err(HelmFailure::Start)?;
    let file = RunKubeconfig::write(&spec.dir, &config).map_err(HelmFailure::Start)?;
    run(HelmCommand {
        binary: spec.binary,
        args: spec.args,
        kubeconfig: file.path().to_path_buf(),
        context: spec.context,
        path_env: spec.path_env,
        timeout: spec.timeout,
    })
    .await
}

#[cfg(test)]
mod tests;
