//! Section 1.3 of the tunnel-subsystem change: `ssh` on `PATH` is a runtime requirement for
//! `SshTunnel` (section 3), which shells out to it. This module gives that code a single
//! fail-fast check with a clear message instead of a raw spawn `NotFound` error.

use std::env;
use std::path::PathBuf;

#[cfg(windows)]
const SSH_EXE_NAME: &str = "ssh.exe";
#[cfg(not(windows))]
const SSH_EXE_NAME: &str = "ssh";

/// Returns the path to `ssh` on `PATH`, or an error message telling the user to install it.
// UNWIRED(#3): called by SshTunnel's spawn path once section 3 lands.
#[allow(dead_code)]
pub fn require_ssh_on_path() -> Result<PathBuf, String> {
    find_on_path(&env::var("PATH").unwrap_or_default()).ok_or_else(|| {
        "`ssh` was not found on PATH. Fernrohr's tunnel subsystem shells out to the system \
         `ssh` client for SSH-tunneled clusters; install OpenSSH and ensure `ssh` is on PATH \
         to use a bound context."
            .to_string()
    })
}

fn find_on_path(path_env: &str) -> Option<PathBuf> {
    env::split_paths(path_env).find_map(|dir| {
        let candidate = dir.join(SSH_EXE_NAME);
        candidate.is_file().then_some(candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Removes its directory on drop so a failed assertion still cleans up.
    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new(name: &str) -> Self {
            let dir = env::temp_dir().join(format!("fernrohr-ssh-path-test-{name}"));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_ssh_on_a_matching_path_entry() {
        let dir = ScratchDir::new("found");
        let ssh_path = dir.0.join(SSH_EXE_NAME);
        fs::write(&ssh_path, b"").unwrap();

        let path_env = dir.0.to_string_lossy().into_owned();
        assert_eq!(find_on_path(&path_env), Some(ssh_path));
    }

    #[test]
    fn reports_none_when_no_path_entry_has_it() {
        let dir = ScratchDir::new("missing");
        let path_env = dir.0.to_string_lossy().into_owned();
        assert_eq!(find_on_path(&path_env), None);
    }

    #[test]
    fn require_ssh_on_path_finds_the_real_system_ssh() {
        // CI runners (macos-latest, ubuntu-latest) and local dev machines ship OpenSSH.
        assert!(
            require_ssh_on_path().is_ok(),
            "expected `ssh` on this machine's real PATH"
        );
    }
}
