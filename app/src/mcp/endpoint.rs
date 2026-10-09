//! The endpoint's files: a Unix-domain socket and a per-launch token beside it,
//! in an owner-only directory (design.md: Per-launch local authorization).
//!
//! [`Endpoint::bind`] runs at app startup. It clears what a crashed run left
//! behind, writes a fresh token, and binds the socket - unless another running
//! app already answers on it, which keeps its endpoint. [`EndpointFiles::remove`]
//! runs at quit, so the next launch starts with a new token and a stale token
//! file never outlives the app that wrote it for long.
//!
//! The token is what an adapter proves it may talk to the app with: only the
//! user (and root) can read it. It is never passed on a command line, where any
//! process could read it.

use super::protocol::EndpointToken;
use crate::util::paths;
use std::fs::{self, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tokio::net::UnixListener;

/// Where an endpoint's socket and token live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EndpointPaths {
    dir: PathBuf,
    pub(super) socket: PathBuf,
    pub(super) token: PathBuf,
}

impl EndpointPaths {
    /// The app's endpoint, in its runtime directory.
    pub(super) fn default_location() -> Self {
        Self::in_dir(paths::runtime_dir().join("mcp"))
    }

    /// An endpoint in `dir`; tests use their own.
    pub(super) fn in_dir(dir: PathBuf) -> Self {
        Self {
            socket: dir.join("endpoint.sock"),
            token: dir.join("endpoint.token"),
            dir,
        }
    }
}

/// Why the app has no endpoint this run.
#[derive(Debug)]
pub(super) enum BindError {
    /// Another running Fernrohr already serves this endpoint.
    InUse,
    /// The directory, token or socket couldn't be set up.
    Io(io::Error),
}

impl From<io::Error> for BindError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A bound endpoint, ready to accept connections.
pub(super) struct Endpoint {
    pub(super) listener: UnixListener,
    pub(super) files: EndpointFiles,
    /// The uid every connecting peer must have: this process's own, read
    /// back from the token file it just created.
    pub(super) owner_uid: u32,
}

/// What a bound endpoint wrote, kept to remove it again.
#[derive(Debug, Clone)]
pub(super) struct EndpointFiles {
    pub(super) paths: EndpointPaths,
    pub(super) token: EndpointToken,
}

impl Endpoint {
    /// Sets up the endpoint at `paths`. Must run inside a tokio runtime
    /// context, which the listener registers with.
    pub(super) fn bind(paths: EndpointPaths) -> Result<Self, BindError> {
        create_private_dir(&paths.dir)?;
        if fs::symlink_metadata(&paths.socket).is_ok() {
            if std::os::unix::net::UnixStream::connect(&paths.socket).is_ok() {
                return Err(BindError::InUse);
            }
            // Nothing listens: a crashed run's socket.
            remove_if_present(&paths.socket)?;
        }
        // A token with no live socket is stale whatever wrote it.
        remove_if_present(&paths.token)?;

        let token = EndpointToken::generate()?;
        write_private(&paths.token, token.expose())?;
        let owner_uid = fs::metadata(&paths.token)?.uid();
        let listener = UnixListener::bind(&paths.socket)?;
        fs::set_permissions(&paths.socket, Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            files: EndpointFiles { paths, token },
            owner_uid,
        })
    }
}

impl EndpointFiles {
    /// Removes the socket and token, if the token on disk is still this run's
    /// - a later run's files are left alone.
    pub(super) fn remove(&self) {
        let ours = EndpointToken::read(&self.paths.token)
            .is_ok_and(|on_disk| on_disk.matches(&self.token));
        if ours {
            let _ = fs::remove_file(&self.paths.socket);
            let _ = fs::remove_file(&self.paths.token);
        }
    }
}

/// Creates `dir` (and its parents) and makes it owner-only. Refuses a symlink
/// or a non-directory in its place.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)?;
    }
    // Owner-only from the moment it exists, so there is no window with the
    // umask's permissions before the `set_permissions` below.
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        result => result?,
    }
    let metadata = fs::symlink_metadata(dir)?;
    if !metadata.is_dir() {
        return Err(io::Error::other(format!(
            "{} is not a directory",
            dir.display()
        )));
    }
    fs::set_permissions(dir, Permissions::from_mode(0o700))
}

/// Writes `contents` to `path` with owner-only permissions from the moment the
/// file exists, through a temporary file renamed into place.
fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    let temp = path.with_extension("tmp");
    remove_if_present(&temp)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    fs::rename(&temp, path)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
