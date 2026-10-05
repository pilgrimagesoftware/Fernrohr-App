//! Starting and stopping a command tunnel's process: finding the program on the
//! login `PATH`, spawning it as the leader of its own process group with stdin held
//! open, and stopping the whole group - `SIGTERM`, a grace period, then `SIGKILL`.
//!
//! The group matters because vendor CLIs are wrappers: `gcloud` is a script that
//! starts `ssh`, which holds the port. Signalling only the pid we spawned would leave
//! that `ssh` running.

use super::output::OutputTail;
use crate::consts::COMMAND_TUNNEL_STOP_GRACE;
use crate::util::pidfile::{self, PidFile};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, ChildStdin, Command};
use tokio::task::JoinHandle;

/// A running command and everything that has to go when it stops.
pub(super) struct Running {
    pub(super) child: Child,
    /// Held so the command never sees end-of-input: one without `-N` would close its
    /// session, and with it the forward.
    _stdin: Option<ChildStdin>,
    /// The readers draining stdout and stderr into the output tail.
    drains: Vec<JoinHandle<()>>,
    /// Removed once the group is gone, so a crash in between still leaves it for the
    /// next launch's sweep.
    pidfile: Option<PidFile>,
}

/// `program` as a path to run: as given when it names a path, otherwise the first
/// executable of that name on `path`.
pub(super) fn find_program(program: &str, path: &str) -> Option<PathBuf> {
    if program.contains('/') {
        return Some(PathBuf::from(program));
    }
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(program))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Spawns `argv` with `path` as its `PATH`, leading its own process group, its
/// output drained into `tail`.
pub(super) fn spawn(argv: &[String], path: &str, tail: &OutputTail) -> Result<Running, String> {
    let (name, args) = argv.split_first().ok_or("the command is empty")?;
    let program = find_program(name, path)
        .ok_or_else(|| format!("{name}: command not found on the login shell's PATH"))?;
    let mut command = Command::new(&program);
    command
        .args(args)
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start {name}: {error}"))?;
    let stdin = child.stdin.take();
    let mut drains = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        drains.push(tokio::spawn(tail.clone().drain(stdout)));
    }
    if let Some(stderr) = child.stderr.take() {
        drains.push(tokio::spawn(tail.clone().drain(stderr)));
    }
    let pidfile = child
        .id()
        .and_then(|pid| PidFile::write_command(pid, &program.display().to_string()).ok());
    Ok(Running {
        child,
        _stdin: stdin,
        drains,
        pidfile,
    })
}

impl Running {
    /// Stops the process group: `SIGTERM`, up to [`COMMAND_TUNNEL_STOP_GRACE`] for the
    /// leader to exit, then `SIGKILL` for whatever of the group is left.
    pub(super) async fn stop(mut self) {
        if let Some(pid) = self.child.id() {
            pidfile::terminate_process_group(pid);
            let _ = tokio::time::timeout(COMMAND_TUNNEL_STOP_GRACE, self.child.wait()).await;
            pidfile::kill_process_group(pid);
        }
        let _ = self.child.wait().await;
        self.finish_output().await;
        drop(self.pidfile.take());
    }

    /// Gives the readers a moment to take in the last of the output - what a
    /// failure reason quotes - once the process has exited, then stops them.
    pub(super) async fn finish_output(&mut self) {
        for mut drain in self.drains.drain(..) {
            if tokio::time::timeout(std::time::Duration::from_millis(500), &mut drain)
                .await
                .is_err()
            {
                drain.abort();
            }
        }
    }

    /// Stops the group from a synchronous `Drop`: on the tokio runtime when there is
    /// one, so the grace period doesn't block the dropping thread; at once otherwise.
    pub(super) fn stop_in_background(self) {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(self.stop());
            }
            Err(_) => {
                if let Some(pid) = self.child.id() {
                    pidfile::kill_process_group(pid);
                }
            }
        }
    }
}
