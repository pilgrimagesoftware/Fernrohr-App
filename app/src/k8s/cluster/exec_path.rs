//! Exec credential plugins found the way a terminal finds them (#178).
//!
//! A kubeconfig names its plugin by bare command - `gke-gcloud-auth-plugin`,
//! `aws` - for `kube` to find on `PATH`. Launched from the Dock or a desktop
//! launcher, the app has the session's minimal `PATH`, where the plugin isn't,
//! and connecting fails with "unable to run auth exec: No such file or
//! directory". So before a client is built from a config, its plugin is
//! pointed at the login shell's `PATH` (`util::login_env`): the bare command
//! becomes the absolute path found there, and the plugin runs with that
//! `PATH` - it often runs other tools in turn, as GKE's plugin runs `gcloud`.
//!
//! Unix only - the module is: a Windows GUI app already has the user's
//! `PATH`.

use kube::Config;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Points `config`'s exec plugin, if it has one, at `path`: a bare `command`
/// found on `path` becomes that absolute path, and the plugin runs with
/// `PATH` set to `path` - unless the kubeconfig sets `PATH` for it itself,
/// which wins. A command that is a path already, or isn't on `path`, is left
/// for `kube` to try as before.
pub(crate) fn use_login_path(config: &mut Config, path: &str) {
    let Some(exec) = config.auth_info.exec.as_mut() else {
        return;
    };
    if let Some(command) = exec.command.as_mut()
        && let Some(found) = find_on(command, path)
    {
        *command = found.to_string_lossy().into_owned();
    }
    let env = exec.env.get_or_insert_with(Vec::new);
    let sets_path = env
        .iter()
        .any(|var| var.get("name").map(String::as_str) == Some("PATH"));
    if !sets_path {
        env.push(HashMap::from([
            ("name".to_string(), "PATH".to_string()),
            ("value".to_string(), path.to_string()),
        ]));
    }
}

/// `command` as found in the first directory of `path` holding an executable
/// by that name - when `command` is a bare name, not a path.
fn find_on(command: &str, path: &str) -> Option<PathBuf> {
    if command.is_empty() || Path::new(command).components().count() != 1 {
        return None;
    }
    std::env::split_paths(path)
        .map(|dir| dir.join(command))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(candidate: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    candidate
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests;
