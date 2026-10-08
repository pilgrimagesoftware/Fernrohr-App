//! Writing renewed tokens back to the kubeconfig, as `kubectl` does - so the
//! issuer's rotated refresh token isn't lost, and `kubectl` keeps working on
//! the same file.
//!
//! Only the user's `auth-provider.config` `id-token` and `refresh-token` are
//! changed. The file is re-serialized (as `kubectl` re-serializes it, comments
//! aren't kept), written beside itself and renamed over, so a crash can't
//! leave it half-written, and keeps its permissions.

use super::{ID_TOKEN, REFRESH_TOKEN, Tokens};
use serde_yaml_ng::Value;
use std::path::{Path, PathBuf};

/// The kubeconfig files `Kubeconfig::read` merges, in its order: `$KUBECONFIG`'s
/// list, else `~/.kube/config`.
pub(in crate::k8s::cluster) fn kubeconfig_files() -> Vec<PathBuf> {
    match std::env::var_os("KUBECONFIG") {
        Some(value) => std::env::split_paths(&value)
            .filter(|path| !path.as_os_str().is_empty())
            .collect(),
        None => dirs::home_dir()
            .map(|home| home.join(".kube").join("config"))
            .into_iter()
            .collect(),
    }
}

/// Saves `tokens` to the first of `files` that defines `user` - the one its
/// credentials were merged from. `Ok(false)` if none does.
pub(super) fn save(files: &[PathBuf], user: &str, tokens: &Tokens) -> Result<bool, String> {
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let mut document: Value = serde_yaml_ng::from_str(&text)
            .map_err(|error| format!("{} isn't valid YAML ({error})", file.display()))?;
        if !set_tokens(&mut document, user, tokens) {
            continue;
        }
        let text = serde_yaml_ng::to_string(&document)
            .map_err(|error| format!("couldn't serialize {} ({error})", file.display()))?;
        replace(file, &text)
            .map_err(|error| format!("couldn't write {} ({error})", file.display()))?;
        return Ok(true);
    }
    Ok(false)
}

/// Sets `user`'s oidc tokens in a kubeconfig `document`; `false` if it doesn't
/// define that user with an oidc auth-provider.
fn set_tokens(document: &mut Value, user: &str, tokens: &Tokens) -> bool {
    let Some(users) = document.get_mut("users").and_then(Value::as_sequence_mut) else {
        return false;
    };
    let Some(config) = users
        .iter_mut()
        .find(|named| named.get("name").and_then(Value::as_str) == Some(user))
        .and_then(|named| named.get_mut("user"))
        .and_then(|info| info.get_mut("auth-provider"))
        .filter(|provider| provider.get("name").and_then(Value::as_str) == Some(super::PROVIDER))
        .and_then(|provider| provider.get_mut("config"))
        .and_then(Value::as_mapping_mut)
    else {
        return false;
    };
    config.insert(ID_TOKEN.into(), tokens.id_token.clone().into());
    if let Some(refresh_token) = &tokens.refresh_token {
        config.insert(REFRESH_TOKEN.into(), refresh_token.clone().into());
    }
    true
}

/// Replaces `file` with `text` atomically, keeping its permissions. A
/// symlinked kubeconfig has its target replaced, the link kept.
fn replace(file: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let file = std::fs::canonicalize(file)?;
    let permissions = std::fs::metadata(&file)?.permissions();
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = file.with_file_name(format!(".{name}.fernrohr-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // Owner-only from the start: the file holds credentials.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options
        .open(&temp)
        .and_then(|mut out| out.write_all(text.as_bytes()).and_then(|()| out.sync_all()))
        .and_then(|()| std::fs::set_permissions(&temp, permissions))
        .and_then(|()| std::fs::rename(&temp, &file));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}
