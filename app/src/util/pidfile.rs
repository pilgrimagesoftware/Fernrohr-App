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
fn process_group_alive(pid: u32) -> bool {
    signal_process_group("-0", pid)
}

#[cfg(not(unix))]
fn process_group_alive(_pid: u32) -> bool {
    false
}

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
}

/// Quit-time teardown: kills every pidfile-recorded forward this *running* process
/// itself spawned, after [`looks_like_live_forward`] confirms it's still a direct
/// child of this process and still looks like one of our `ssh -N -L` forwards. Wired
/// into `util::shell::init`'s `cx.on_app_quit` hook, since GPUI's quit path never
/// runs the `Drop` glue that would otherwise kill these forwards on its own. Returns
/// the number of pidfiles found.
pub(crate) fn kill_live_forwards() -> usize {
    sweep_dir(&pid_dir(), looks_like_live_forward)
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// Points `pid_dir()` at a fresh scratch directory for the duration of one test
    /// by overriding `HOME`/`XDG_CACHE_HOME` isn't practical per-test (global env,
    /// tests run concurrently) - instead these tests drive the pure/private
    /// functions (`sweep_dir`, the `looks_like_*` predicates, `kill_process_group`,
    /// `process_group_alive`) directly against an explicit scratch directory rather
    /// than through the process-wide `pid_dir()`/`sweep_stale()`/`kill_live_forwards()`
    /// trio.
    fn scratch_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "fernrohr-pidfile-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("scratch dir should be creatable");
        dir
    }

    fn write_pidfile(dir: &Path, pid: u32) -> PathBuf {
        let path = dir.join(format!("{pid}.pid"));
        fs::write(&path, pid.to_string()).expect("pidfile should be writable");
        path
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

    /// A script that survives as a distinct, long-lived process under whatever name
    /// `exec -a` gives it: backgrounding `sleep` and `wait`-ing on it (rather than a
    /// bare `sleep`) stops bash's "replace myself with my one last simple command"
    /// optimization from discarding the renamed argv[0] by exec'ing straight into
    /// `sleep` itself.
    const SSH_STAND_IN_SCRIPT: &str = "sleep 20 & wait";

    /// A harmless stand-in for a live `SshTransport`-spawned `ssh`: `exec -a ssh`
    /// renames it to `ssh` and appends `-N`/`-L` to its own argv, so it satisfies
    /// [`is_ssh_forward_command`] without needing a real `ssh` binary or opening a
    /// port. Spawned directly (`process_group(0)`) as a child of *this* test process,
    /// for the [`looks_like_live_forward`]/[`kill_live_forwards`] path.
    fn spawn_ssh_stand_in() -> std::process::Child {
        use std::os::unix::process::CommandExt as _;
        Command::new("bash")
            .args([
                "-c",
                &format!("exec -a ssh bash -c '{SSH_STAND_IN_SCRIPT}' -N -L"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("bash should spawn")
    }

    /// The same stand-in, but orphaned to pid 1 before this returns - mirroring how a
    /// crashed app leaves a real `ssh` forward reparented to init while `ssh` itself
    /// keeps running unaffected. A throwaway launcher backgrounds the stand-in into
    /// its own process group (`set -m`, bash's per-job process group in monitor mode)
    /// and exits immediately without waiting on it; `Command::output` blocks until
    /// that launcher exits, which is exactly when the stand-in gets reparented. The
    /// backgrounded stand-in's own stdout/stderr are redirected to `/dev/null` rather
    /// than left inherited from the launcher: otherwise the stand-in - which outlives
    /// the launcher by design - would keep the launcher's stdout pipe open for the
    /// full 20 seconds of [`SSH_STAND_IN_SCRIPT`], and `Command::output` (below) reads
    /// that pipe until EOF, not just until the launcher itself exits.
    /// Returns the stand-in's pid, which is also its pgid.
    fn spawn_orphaned_ssh_stand_in() -> u32 {
        let script = format!(
            "set -m; exec -a ssh bash -c '{SSH_STAND_IN_SCRIPT}' -N -L >/dev/null 2>&1 & echo $!"
        );
        let output = Command::new("bash")
            .args(["-c", &script])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .expect("launcher should run");
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .expect("launcher should print the stand-in's pid")
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

    /// Regression: without `--`, procps-ng `kill -KILL -<pid>` signalled every process
    /// the user owned, taking the Linux CI runner down with it. An unrelated process in
    /// its own group must survive a kill aimed at a different group.
    #[test]
    fn kill_process_group_leaves_other_groups_alone() {
        let mut target = spawn_sleep_in_its_own_group();
        let mut bystander = spawn_sleep_in_its_own_group();

        kill_process_group(target.id());
        target.wait().expect("target should be waitable after kill");

        assert!(process_group_alive(bystander.id()));
        assert!(
            bystander
                .try_wait()
                .expect("bystander should be pollable")
                .is_none()
        );

        kill_process_group(bystander.id());
        let _ = bystander.wait();
    }

    #[test]
    fn signal_process_group_refuses_own_group_and_broadcast_pgids() {
        assert!(!process_group_alive(0));
        assert!(!process_group_alive(1));
    }

    #[test]
    fn sweep_ignores_non_pid_files_and_empty_dirs() {
        let dir = scratch_dir("empty");
        fs::write(dir.join("notes.txt"), "not a pidfile").unwrap();

        assert_eq!(sweep_dir(&dir, looks_like_orphaned_forward), 0);
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

    #[test]
    fn is_ssh_forward_command_matches_only_an_ssh_dash_n_dash_l_invocation() {
        assert!(is_ssh_forward_command(
            "/usr/bin/ssh -N -L 15000:10.0.0.1:443 user@bastion"
        ));
        assert!(!is_ssh_forward_command("/usr/bin/ssh user@bastion"));
        assert!(!is_ssh_forward_command("/usr/sbin/sshd -N -L"));
        assert!(!is_ssh_forward_command("sleep 30 -N -L"));
        assert!(!is_ssh_forward_command(""));
    }

    /// Section 6.2's quit-time gap: `sweep_dir` with [`looks_like_live_forward`] is
    /// exactly what `kill_live_forwards` runs from the app-quit hook. Proves it kills
    /// a live, still-parented-by-us stand-in and cleans up its pidfile.
    #[test]
    fn live_forward_predicate_kills_a_live_stand_in_and_removes_its_pidfile() {
        let mut child = spawn_ssh_stand_in();
        let pid = child.id();
        let dir = scratch_dir("kill-live");
        let pidfile_path = write_pidfile(&dir, pid);

        assert_eq!(sweep_dir(&dir, looks_like_live_forward), 1);
        assert!(!pidfile_path.exists());
        let status = child.wait().expect("child should be waitable after kill");
        assert!(!status.success());

        let _ = fs::remove_dir_all(&dir);
    }

    /// The startup-sweep counterpart: proves `sweep_stale`'s predicate kills a
    /// genuinely orphaned (ppid 1) stand-in - the crash-recovery case - and cleans up
    /// its pidfile.
    #[test]
    fn orphaned_forward_predicate_kills_an_orphaned_stand_in_and_removes_its_pidfile() {
        let pid = spawn_orphaned_ssh_stand_in();
        assert!(
            process_group_alive(pid),
            "the stand-in should still be running, just orphaned"
        );

        let dir = scratch_dir("sweep-orphan");
        let pidfile_path = write_pidfile(&dir, pid);

        assert_eq!(sweep_dir(&dir, looks_like_orphaned_forward), 1);
        assert!(!pidfile_path.exists());
        assert!(
            !process_group_alive(pid),
            "the orphaned stand-in should be dead after the sweep"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// The pid-reuse guard: a pidfile naming a live, direct child of this process (so
    /// it passes every check [`looks_like_live_forward`] makes except the command
    /// line) must not be killed - and the same holds for [`looks_like_orphaned_forward`]
    /// against a non-`ssh` command. Proves the command-line check, not just liveness
    /// or parentage, is what gates the kill.
    #[test]
    fn a_non_ssh_pid_is_never_killed_by_either_predicate() {
        let mut child = spawn_sleep_in_its_own_group();
        let pid = child.id();
        let dir = scratch_dir("mismatch");
        let pidfile_path = write_pidfile(&dir, pid);

        assert_eq!(
            sweep_dir(&dir, looks_like_live_forward),
            1,
            "still counted as a found pidfile"
        );
        assert!(
            process_group_alive(pid),
            "a non-ssh command must never be killed, even though it is a live, \
             directly-parented-by-us process in its own group"
        );
        assert!(
            !pidfile_path.exists(),
            "the stale/mismatched pidfile is still removed either way"
        );

        kill_process_group(pid);
        let _ = child.wait();
        let _ = fs::remove_dir_all(&dir);
    }
}
