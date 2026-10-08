//! One renewal at a time per user. Dex rotates the refresh token on every
//! renewal and refuses the old one after it, so two renewals racing - two
//! connections, a reconnect and a manual connect, two Fernrohr windows - would
//! each spend the same refresh token, and the later write would clobber the
//! rotated one: the user is locked out until they log in again.
//!
//! So a renewal holds an in-process async lock keyed by the kubeconfig file
//! and user, and an advisory file lock (`flock`) for other processes, from
//! re-reading the file through writing it back. The file lock is on a sidecar
//! (`.config.fernrohr-lock`), not the kubeconfig: that is replaced by rename,
//! which would leave a lock on the old inode, and `kubectl`'s own `config.lock`
//! is an exclusive-create marker this must never leave behind.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

/// Held for the renewal; both locks are released when it drops.
pub(super) struct RenewalLock {
    _in_process: tokio::sync::OwnedMutexGuard<()>,
    _file: Option<std::fs::File>,
}

type Key = (Option<PathBuf>, String);

/// One async lock per (kubeconfig file, user) renewed in this process. Never
/// pruned: there is one entry per OIDC user ever renewed, a handful at most.
static LOCKS: LazyLock<Mutex<HashMap<Key, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(Default::default);

/// Waits for `user`'s renewal lock: in-process always, and on `file` (the
/// kubeconfig defining the user) when there is one.
pub(super) async fn acquire(file: Option<&Path>, user: &str) -> Result<RenewalLock, String> {
    let file = file.map(|file| std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf()));
    let in_process = LOCKS
        .lock()
        .entry((file.clone(), user.to_string()))
        .or_default()
        .clone();
    let in_process = in_process.lock_owned().await;
    let file = match file {
        Some(file) => Some(lock_file(file).await?),
        None => None,
    };
    Ok(RenewalLock {
        _in_process: in_process,
        _file: file,
    })
}

/// The sidecar beside `kubeconfig`, locked exclusively. `flock` blocks, so it
/// is taken on a blocking thread.
async fn lock_file(kubeconfig: PathBuf) -> Result<std::fs::File, String> {
    let name = kubeconfig
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path = kubeconfig.with_file_name(format!(".{name}.fernrohr-lock"));
    tokio::task::spawn_blocking(move || {
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(false).write(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let file = options.open(&path)?;
        file.lock()?;
        Ok::<_, std::io::Error>(file)
    })
    .await
    .map_err(|error| format!("the kubeconfig lock couldn't be taken ({error})"))?
    .map_err(|error| format!("the kubeconfig lock couldn't be taken ({error})"))
}
