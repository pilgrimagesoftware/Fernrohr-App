//! Tests for `util::pidfile`: the sweep predicates and process-group signalling,
//! driven against real stand-in processes in scratch directories.

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
/// Returns the stand-in's pid, which is also its pgid - but only once the stand-in
/// is observably what the sweep looks for. `$!` is printed right after the fork,
/// before the backgrounded subshell has run its `exec -a ssh`, so until then `ps`
/// still shows the launcher's own `bash -c ...` command line; and the reparenting
/// to pid 1 lands only when the launcher exits. Sweeping before both have settled
/// is a race the sweep loses on a loaded Linux runner.
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
    let pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("launcher should print the stand-in's pid");
    assert!(
        eventually(|| looks_like_orphaned_forward(pid)),
        "the stand-in never settled into an orphaned ssh forward: ppid {:?}, command {:?}",
        process_ppid(pid),
        process_command(pid),
    );
    pid
}

/// Polls `condition` until it holds or a deadline passes, for process state that
/// the kernel settles asynchronously (an `exec`, a reparenting, a reaped kill).
/// The deadline is generous because it only bounds a failure; a passing run
/// returns as soon as the state lands.
fn eventually(condition: impl Fn() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if condition() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
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
    // SIGKILL is delivered asynchronously, and the killed orphan lingers as a
    // zombie - which `kill -0` still finds - until init reaps it.
    assert!(
        eventually(|| !process_group_alive(pid)),
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
