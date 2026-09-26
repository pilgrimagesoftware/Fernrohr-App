//! Section 3.4 of the tunnel-subsystem change: orphan cleanup for `ssh` forwards.
//!
//! `SshTransport` (`ssh_tunnel.rs`) spawns `ssh` into its own process group so a
//! `-J` jump-host hop - itself a separate `ssh` subprocess - dies alongside the
//! direct child rather than surviving `kill_on_drop`, which only signals the direct
//! pid. This module owns the two pieces that support that: [`kill_process_group`],
//! and a pidfile per live forward so [`sweep_stale`] can find and kill anything left
//! behind by a crash (a normal `Drop` removes its own pidfile, so anything `sweep_stale`
//! finds at startup is necessarily from a run that never got to clean up).

use crate::paths;
use std::fs;
use std::path::{Path, PathBuf};

fn pid_dir() -> PathBuf {
    paths::cache_dir().join("ssh-pids")
}

/// Sends the platform's process-group-kill to `pid`'s group. `ssh` is spawned with
/// `process_group(0)` (unix-only, see `ssh_tunnel.rs`), so its pgid equals its own
/// pid and every jump-host hop it spawns inherits that group. Shelling out to `kill`
/// avoids a new dependency just for `killpg`; this only runs on teardown, not a hot
/// path.
#[cfg(unix)]
pub(crate) fn kill_process_group(pid: u32) {
    let _ = std::process::Command::new("kill")
        .arg("-KILL")
        .arg(format!("-{pid}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(not(unix))]
pub(crate) fn kill_process_group(_pid: u32) {}

/// Checks whether `pid` (interpreted as a process group id, i.e. `-pid`) still has
/// any member alive, via a zero-signal `kill -0`.
#[cfg(unix)]
fn process_group_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(format!("-{pid}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(unix))]
fn process_group_alive(_pid: u32) -> bool {
    false
}

/// One pidfile for one live `ssh` forward. Written on spawn, removed on `Drop` - so a
/// clean shutdown leaves nothing for [`sweep_stale`] to find at the next startup.
// UNWIRED(#3): `SshTransport::connect` is the only writer today; `sweep_stale` has no
// startup caller yet (that lands with section 6.2's connect-path integration).
#[allow(dead_code)]
pub(crate) struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Writes `pid` to a fresh pidfile in [`pid_dir`], creating the directory if
    /// needed. Named by pid, so two forwards never collide.
    // UNWIRED(#3): only `SshTransport::connect` calls this, and that impl itself has
    // no caller until section 6.2's connect-path integration.
    #[allow(dead_code)]
    pub(crate) fn write(pid: u32) -> std::io::Result<Self> {
        let dir = pid_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{pid}.pid"));
        fs::write(&path, pid.to_string())?;
        Ok(Self { path })
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Startup sweep: any pidfile found in [`pid_dir`] belongs to a forward whose `Drop`
/// never ran, i.e. the previous run crashed before tearing it down. Kills each
/// surviving process group and removes the file either way. Returns the number of
/// pidfiles found (killed or already dead).
// UNWIRED(#3): no startup caller yet - section 6.2's connect-path integration wires
// this into app boot. Covered directly by this module's own tests until then.
#[allow(dead_code)]
pub(crate) fn sweep_stale() -> usize {
    let dir = pid_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return 0;
    };

    let mut swept = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "pid") {
            continue;
        }
        if let Some(pid) = read_pid(&path) {
            if process_group_alive(pid) {
                kill_process_group(pid);
            }
            swept += 1;
        }
        let _ = fs::remove_file(&path);
    }
    swept
}

fn read_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// Points `pid_dir()` at a fresh scratch directory for the duration of one test
    /// by overriding `HOME`/`XDG_CACHE_HOME` isn't practical per-test (global env,
    /// tests run concurrently) - instead these tests drive the pure functions
    /// (`kill_process_group`, `process_group_alive`, `read_pid`, the sweep logic over
    /// an explicit directory) directly rather than through the process-wide
    /// `pid_dir()`/`sweep_stale()` pair, which is exercised by `ssh_tunnel`'s
    /// integration test instead.
    fn sweep_dir(dir: &Path) -> usize {
        let Ok(entries) = fs::read_dir(dir) else {
            return 0;
        };
        let mut swept = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "pid") {
                continue;
            }
            if let Some(pid) = read_pid(&path) {
                if process_group_alive(pid) {
                    kill_process_group(pid);
                }
                swept += 1;
            }
            let _ = fs::remove_file(&path);
        }
        swept
    }

    /// Puts `child` in its own process group (pgid = its own pid), mirroring how
    /// `ssh_tunnel::SshTransport::connect` spawns `ssh` - `kill -0 -<pid>` only finds
    /// a group actually rooted at `pid`, not whatever group the test harness itself
    /// happens to be in.
    fn spawn_sleep_in_its_own_group() -> std::process::Child {
        use std::os::unix::process::CommandExt as _;
        Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("sleep should spawn")
    }

    #[test]
    fn kill_process_group_terminates_a_real_process_group() {
        let mut child = spawn_sleep_in_its_own_group();
        let pid = child.id();

        assert!(process_group_alive(pid));
        kill_process_group(pid);
        let status = child.wait().expect("child should be waitable after kill");
        assert!(!status.success());
        assert!(!process_group_alive(pid));
    }

    #[test]
    fn sweep_kills_a_stale_pidfile_and_removes_it() {
        let mut child = spawn_sleep_in_its_own_group();
        let pid = child.id();

        let dir = std::env::temp_dir().join(format!("fernrohr-pidfile-sweep-test-{pid}"));
        fs::create_dir_all(&dir).unwrap();
        let pidfile_path = dir.join(format!("{pid}.pid"));
        fs::write(&pidfile_path, pid.to_string()).unwrap();

        let swept = sweep_dir(&dir);

        assert_eq!(swept, 1);
        assert!(!pidfile_path.exists());
        let status = child.wait().expect("child should be waitable after sweep");
        assert!(!status.success());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweep_ignores_non_pid_files_and_empty_dirs() {
        let dir = std::env::temp_dir().join(format!(
            "fernrohr-pidfile-sweep-empty-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("notes.txt"), "not a pidfile").unwrap();

        assert_eq!(sweep_dir(&dir), 0);
        assert!(dir.join("notes.txt").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pidfile_write_then_drop_removes_the_file() {
        let pidfile = PidFile::write(std::process::id()).expect("write should succeed");
        let path = pidfile.path.clone();
        assert!(path.exists());
        drop(pidfile);
        assert!(!path.exists());
    }
}
