//! The kubeconfig one `helm` run reads (`helm-releases` D8): a single context
//! whose cluster is the connection's *effective* endpoint - through its
//! tunnel when it has one - and whose user is the context's own, copied
//! unchanged so `helm` authenticates exactly as the app does.
//!
//! Each run gets a fresh file under the cache dir, created 0600 and removed
//! when the run ends ([`RunKubeconfig`]'s `Drop`). A crash can leave one
//! behind, holding credentials, so startup sweeps any whose run can no longer
//! be going ([`sweep_stale`]). Nothing here touches the user's own
//! kubeconfig or environment.

use crate::consts::HELM_ACTION_TIMEOUT;
use crate::k8s::cluster::tunnel::TunnelRoute;
use kube::config::Kubeconfig;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// How a connection reaches its API server, so `helm` can be sent the same
/// way - the kubeconfig cluster as is, or through the bound tunnel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Endpoint {
    /// No tunnel, or a manual one the client reaches by its own address.
    Direct,
    /// The local port *is* the API server (an SSH tunnel, or a command
    /// tunnel in forward mode): connect there, verifying the real host name.
    Rewrite(SocketAddr),
    /// The local port is an HTTP proxy (a command tunnel in proxy mode):
    /// connect to the real server through it.
    Proxy(SocketAddr),
}

impl Endpoint {
    /// The endpoint for a connection with `tunnel` - its route and local
    /// address - or none.
    pub(crate) fn of(tunnel: Option<(TunnelRoute, SocketAddr)>) -> Self {
        match tunnel {
            None | Some((TunnelRoute::Direct, _)) => Self::Direct,
            Some((TunnelRoute::Rewrite, local)) => Self::Rewrite(local),
            Some((TunnelRoute::Proxy, local)) => Self::Proxy(local),
        }
    }
}

/// `source`'s `context`, alone, with its cluster pointed at `endpoint` and
/// its user stanza unchanged - exec plugins, tokens and client certificates
/// included.
pub(crate) fn for_run(
    source: &Kubeconfig,
    context: &str,
    endpoint: Endpoint,
) -> Result<Kubeconfig, String> {
    let named = source
        .contexts
        .iter()
        .find(|named| named.name == context)
        .ok_or_else(|| format!("The kubeconfig has no context named {context}."))?;
    let context_spec = named
        .context
        .clone()
        .ok_or_else(|| format!("The context {context} is empty."))?;
    let mut cluster = source
        .clusters
        .iter()
        .find(|cluster| cluster.name == context_spec.cluster)
        .cloned()
        .ok_or_else(|| {
            format!(
                "The kubeconfig has no cluster named {}.",
                context_spec.cluster
            )
        })?;
    let user = match &context_spec.user {
        Some(user) => Some(
            source
                .auth_infos
                .iter()
                .find(|auth| &auth.name == user)
                .cloned()
                .ok_or_else(|| format!("The kubeconfig has no user named {user}."))?,
        ),
        None => None,
    };
    if let Some(spec) = cluster.cluster.as_mut() {
        match endpoint {
            Endpoint::Direct => {}
            Endpoint::Rewrite(local) => {
                let server = spec
                    .server
                    .as_deref()
                    .ok_or_else(|| format!("The cluster {} has no server.", cluster.name))?;
                let url = url::Url::parse(server).map_err(|error| {
                    format!("The cluster's server {server} isn't a URL: {error}")
                })?;
                if spec.tls_server_name.is_none() {
                    spec.tls_server_name = url.host_str().map(str::to_string);
                }
                spec.server = Some(format!("{}://{local}", url.scheme()));
                // The tunnel is the server now; a proxy would route around it.
                spec.proxy_url = None;
            }
            Endpoint::Proxy(local) => spec.proxy_url = Some(format!("http://{local}")),
        }
    }
    Ok(Kubeconfig {
        clusters: vec![cluster],
        auth_infos: user.into_iter().collect(),
        // The context itself unchanged: its cluster, user and namespace.
        contexts: vec![named.clone()],
        current_context: Some(context.to_string()),
        api_version: Some("v1".into()),
        kind: Some("Config".into()),
        ..Kubeconfig::default()
    })
}

/// Where run kubeconfigs are written.
pub(super) fn dir() -> PathBuf {
    crate::util::paths::cache_dir().join("helm-kubeconfigs")
}

/// A run's kubeconfig on disk, removed when dropped - however the run ends.
pub(crate) struct RunKubeconfig {
    path: PathBuf,
}

impl RunKubeconfig {
    /// Writes `config` to a new file in `dir`, readable by the user alone
    /// from the moment it exists: created 0600, never widened.
    pub(crate) fn write(dir: &Path, config: &Kubeconfig) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let yaml = serde_yaml_ng::to_string(config)
            .map_err(|error| format!("Couldn't write a kubeconfig for helm: {error}"))?;
        create_private_dir(dir)?;
        let path = dir.join(format!(
            "{}-{}.yaml",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = private_file(&path)
            .map_err(|error| format!("Couldn't create {}: {error}", path.display()))?;
        let run = Self { path };
        std::io::Write::write_all(&mut file, yaml.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Couldn't write {}: {error}", run.path.display()))?;
        Ok(run)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RunKubeconfig {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("couldn't remove {}: {error}", self.path.display());
        }
    }
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|error| format!("Couldn't create {}: {error}", dir.display()))
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("Couldn't create {}: {error}", dir.display()))
}

#[cfg(unix)]
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Removes the run kubeconfigs a crashed run left behind - each holds a copy
/// of its context's credentials. Called at startup. A file is left only while
/// its run could still be going: written by a live app process, and younger
/// than the longest a run may take. Returns how many were removed.
pub(crate) fn sweep_stale() -> usize {
    sweep(&dir(), SystemTime::now(), process_alive)
}

/// [`sweep_stale`] over `dir` as of `now`, `alive` telling whether the
/// process that wrote a file is still running.
pub(super) fn sweep(dir: &Path, now: SystemTime, alive: impl Fn(u32) -> bool) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let oldest_live = now
        .checked_sub(HELM_ACTION_TIMEOUT + Duration::from_secs(60))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    entries
        .flatten()
        .filter(|entry| {
            let writer = entry
                .file_name()
                .to_str()
                .and_then(|name| name.split('-').next()?.parse::<u32>().ok());
            let modified = entry.metadata().and_then(|meta| meta.modified()).ok();
            let may_be_running = writer.is_some_and(&alive)
                && modified.is_some_and(|modified| modified >= oldest_live);
            !may_be_running
        })
        .filter(|entry| std::fs::remove_file(entry.path()).is_ok())
        .count()
}

/// Whether process `pid` exists (`kill -0`), the way `util::pidfile` checks
/// its groups.
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    pid == std::process::id()
        || std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
}

#[cfg(not(unix))]
fn process_alive(pid: u32) -> bool {
    pid == std::process::id()
}

#[cfg(test)]
mod tests;
