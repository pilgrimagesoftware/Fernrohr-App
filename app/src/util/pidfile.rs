//! Section 3.4 of the tunnel-subsystem change: orphan cleanup for `ssh` forwards.
//!
//! `SshTransport` (`ssh_tunnel.rs`) spawns `ssh` into its own process group so a
//! `-J` jump-host hop - itself a separate `ssh` subprocess - dies alongside the
//! direct child rather than surviving `kill_on_drop`, which only signals the direct
//! pid. This module owns the two pieces that support that: [`kill_process_group`],
//! and a pidfile per live forward, written by [`PidFile::write`] and removed by its
//! own `Drop`, so two independent sweeps can find and kill anything a `Drop` never
//! got to run for:
//!
//! - [`sweep_stale`], run once at startup (`main.rs`, before any tunnel is acquired):
//!   a pidfile surviving to the next launch means the *previous* run crashed or was
//!   killed before it could tear its forward down.
//! - [`kill_live_forwards`], run from this run's own `cx.on_app_quit` hook
//!   (`util::shell::init`): GPUI's quit path tears down windows but never runs the
//!   `Drop` glue on app-scoped globals (`ClusterRegistry`, and so every live
//!   `RegistryHandle`/`SshTunnel`/`SshTransport` it holds), so nothing would
//!   otherwise kill this run's own forwards before the process exits.
//!
//! Both sweeps only ever kill a pidfile's process group after verifying it still
//! looks like one of *our* `ssh -N -L` forwards (command line) and still has the
//! parent we'd expect (gone, for a startup sweep's crash recovery; us, for a live
//! quit-time kill) - a bare "is something still running at this pid" check would
//! risk killing an unrelated process that happened to reuse a recycled pid.
//!
//! `command-tunnels` adds a second directory, `cmd-pids`, for command tunnels: their
//! program is the user's, not `ssh`, so each pidfile also records its program's
//! basename and the sweeps check for that instead (`command`).

use crate::util::paths;
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
    let _ = signal_process_group("-KILL", pid);
}

#[cfg(not(unix))]
pub(crate) fn kill_process_group(_pid: u32) {}

/// Checks whether `pid` (interpreted as a process group id, i.e. `-pid`) still has
/// any member alive, via a zero-signal `kill -0`.
#[cfg(unix)]
pub(crate) fn process_group_alive(pid: u32) -> bool {
    signal_process_group("-0", pid)
}

#[cfg(not(unix))]
pub(crate) fn process_group_alive(_pid: u32) -> bool {
    false
}

/// Sends `SIGTERM` to `pid`'s process group - the graceful first half of stopping a
/// command tunnel, whose program may be a wrapper that has to pass the signal on.
#[cfg(unix)]
pub(crate) fn terminate_process_group(pid: u32) {
    let _ = signal_process_group("-TERM", pid);
}

#[cfg(not(unix))]
pub(crate) fn terminate_process_group(_pid: u32) {}

/// Runs `kill <signal> -- -<pid>` and reports whether it succeeded.
///
/// The `--` is load-bearing. Without it, procps-ng's `/usr/bin/kill` (Ubuntu, Debian)
/// misparses `kill -KILL -<pid>` and signals every process the user can reach, which on
/// a CI runner includes the runner agent itself. BSD `kill` on macOS accepts either
/// form. pgids 0 and 1 are refused outright: `-0` is the caller's own process group
/// and `-1` is every process, and a corrupt pidfile must not be able to name either.
#[cfg(unix)]
fn signal_process_group(signal: &str, pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    std::process::Command::new("kill")
        .arg(signal)
        .arg("--")
        .arg(format!("-{pid}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// One pidfile for one live `ssh` forward. Written by [`Self::write`] on spawn,
/// removed by `Drop` - so a clean teardown (whether the forward simply stops, or
/// this run's own quit hook kills it via [`kill_live_forwards`]) leaves nothing for
/// the *next* startup's [`sweep_stale`] to find.
pub(crate) struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Writes `pid` to a fresh pidfile in [`pid_dir`], creating the directory if
    /// needed. Named by pid, so two forwards never collide.
    pub(crate) fn write(pid: u32) -> std::io::Result<Self> {
        let dir = pid_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{pid}.pid"));
        fs::write(&path, pid.to_string())?;
        Ok(Self { path })
    }

    /// Writes a command tunnel's pidfile: `pid`, then its program's basename, which
    /// the sweeps check the running process against (`command`).
    pub(crate) fn write_command(pid: u32, program: &str) -> std::io::Result<Self> {
        command::write(pid, program)
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Startup sweep: any pidfile found in [`pid_dir`] belongs to a forward whose `Drop`
/// never ran, i.e. the previous run crashed or was killed before it could tear
/// itself down. Kills each such process group - after [`looks_like_orphaned_forward`]
/// confirms it's still ours, not a recycled pid - and removes the file either way.
/// Returns the number of pidfiles found (killed, refused, or already dead).
pub(crate) fn sweep_stale() -> usize {
    sweep_dir(&pid_dir(), looks_like_orphaned_forward)
        + command::sweep_dir(&command::pid_dir(), command::looks_like_orphaned)
}

/// Quit-time teardown: kills every pidfile-recorded forward this *running* process
/// itself spawned, after [`looks_like_live_forward`] confirms it's still a direct
/// child of this process and still looks like one of our `ssh -N -L` forwards. Wired
/// into `util::shell::init`'s `cx.on_app_quit` hook, since GPUI's quit path never
/// runs the `Drop` glue that would otherwise kill these forwards on its own. Returns
/// the number of pidfiles found.
pub(crate) fn kill_live_forwards() -> usize {
    sweep_dir(&pid_dir(), looks_like_live_forward)
        + command::sweep_dir(&command::pid_dir(), command::looks_like_live)
}

/// Shared sweep loop behind [`sweep_stale`] and [`kill_live_forwards`]: every `*.pid`
/// file in `dir` is removed unconditionally (stale either way, once sweept), but its
/// process group is only killed when `should_kill` - the caller's parent/cmdline
/// check - agrees, and only while it's still alive.
fn sweep_dir(dir: &Path, should_kill: impl Fn(u32) -> bool) -> usize {
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
            if process_group_alive(pid) && should_kill(pid) {
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

/// True when `pid` is still running as a direct child of pid 1 - i.e. whatever
/// spawned it (a previous run of this app) is gone - and its command line looks like
/// one of our `ssh -N -L` forwards. Guards [`sweep_stale`] against ever killing a
/// process that merely reused a pid one of our pidfiles still names.
#[cfg(unix)]
fn looks_like_orphaned_forward(pid: u32) -> bool {
    process_ppid(pid) == Some(1) && is_our_ssh_forward(pid)
}

#[cfg(not(unix))]
fn looks_like_orphaned_forward(_pid: u32) -> bool {
    false
}

/// True when `pid` is still running as a direct child of *this* process and its
/// command line looks like one of our `ssh -N -L` forwards. Guards
/// [`kill_live_forwards`] the same way [`looks_like_orphaned_forward`] guards
/// [`sweep_stale`], just against this run's own parentage rather than pid 1's.
#[cfg(unix)]
fn looks_like_live_forward(pid: u32) -> bool {
    process_ppid(pid) == Some(std::process::id()) && is_our_ssh_forward(pid)
}

#[cfg(not(unix))]
fn looks_like_live_forward(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn is_our_ssh_forward(pid: u32) -> bool {
    process_command(pid).is_some_and(|command| is_ssh_forward_command(&command))
}

/// The command-line half of [`is_our_ssh_forward`], split out as a pure function so
/// it's testable without spawning anything. The program name must be `ssh` exactly
/// (a path component match, not a substring one - `sshd` or `my-ssh-wrapper` don't
/// count), and the arguments must include both `-N` and `-L`, mirroring the flags
/// `SshTunnelConfig::args` always passes for a forward (as opposed to, say, the
/// Tunnels window's `-N`-less connectivity test).
fn is_ssh_forward_command(command: &str) -> bool {
    let Some(program) = command.split_whitespace().next() else {
        return false;
    };
    let is_ssh = Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        == Some("ssh");
    is_ssh && command.contains("-N") && command.contains("-L")
}

/// `ps -o ppid=`'s stdout for `pid`, parsed - `None` if `pid` no longer exists or
/// `ps` couldn't be run.
#[cfg(unix)]
fn process_ppid(pid: u32) -> Option<u32> {
    let output = std::process::Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// `ps -o args=`'s stdout for `pid`, i.e. its full command line - `-ww` disables
/// `ps`'s terminal-width truncation, since a real `ssh -J` invocation's argv is
/// easily wider than 80 columns. `None` if `pid` no longer exists, `ps` couldn't be
/// run, or it printed nothing.
#[cfg(unix)]
fn process_command(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-ww", "-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!command.is_empty()).then_some(command)
}

mod command;

#[cfg(all(test, unix))]
mod tests;
