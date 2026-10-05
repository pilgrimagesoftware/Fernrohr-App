//! Command tunnels' pidfiles, in their own `cmd-pids` directory: the pid, then the
//! program's basename. A command tunnel runs whatever the user configured, so the
//! `ssh -N -L` check can't tell ours from a recycled pid; the recorded program can.
//!
//! The check is loose about *where* the program appears: a wrapper script runs as
//! its interpreter (`/bin/sh /opt/sdk/bin/gcloud ...`) and may re-exec into a
//! script of the same stem (`python3 .../gcloud.py ...`), so the program's name, or
//! that name with a script extension, may be any of the first three arguments. The
//! parent-pid check still has to pass too.

use super::*;

pub(super) fn pid_dir() -> PathBuf {
    paths::cache_dir().join("cmd-pids")
}

pub(super) fn write(pid: u32, program: &str) -> std::io::Result<PidFile> {
    write_in(&pid_dir(), pid, program)
}

pub(super) fn write_in(dir: &Path, pid: u32, program: &str) -> std::io::Result<PidFile> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{pid}.pid"));
    fs::write(&path, format!("{pid}\n{}", basename(program)))?;
    Ok(PidFile { path })
}

fn basename(program: &str) -> &str {
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
}

/// The command-tunnel twin of the `ssh` sweep loop: every `*.pid` file is removed,
/// and its process group killed only while alive and when `should_kill` agrees for
/// the recorded program.
pub(super) fn sweep_dir(dir: &Path, should_kill: impl Fn(u32, &str) -> bool) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut swept = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "pid") {
            continue;
        }
        if let Some((pid, program)) = read(&path) {
            if process_group_alive(pid) && should_kill(pid, &program) {
                kill_process_group(pid);
            }
            swept += 1;
        }
        let _ = fs::remove_file(&path);
    }
    swept
}

fn read(path: &Path) -> Option<(u32, String)> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let pid = lines.next()?.trim().parse().ok()?;
    let program = lines.next()?.trim().to_string();
    (!program.is_empty()).then_some((pid, program))
}

/// Orphaned to pid 1 - the run that started it is gone - and still running `program`.
#[cfg(unix)]
pub(super) fn looks_like_orphaned(pid: u32, program: &str) -> bool {
    process_ppid(pid) == Some(1) && runs(pid, program)
}

#[cfg(not(unix))]
pub(super) fn looks_like_orphaned(_pid: u32, _program: &str) -> bool {
    false
}

/// A direct child of this process, still running `program`.
#[cfg(unix)]
pub(super) fn looks_like_live(pid: u32, program: &str) -> bool {
    process_ppid(pid) == Some(std::process::id()) && runs(pid, program)
}

#[cfg(not(unix))]
pub(super) fn looks_like_live(_pid: u32, _program: &str) -> bool {
    false
}

#[cfg(unix)]
fn runs(pid: u32, program: &str) -> bool {
    process_command(pid).is_some_and(|command| is_command_for(&command, program))
}

/// The extensions a wrapper's re-exec'd script may carry.
const SCRIPT_EXTENSIONS: [&str; 5] = ["py", "sh", "rb", "pl", "js"];

/// Whether `command` (a `ps` command line) runs `program`: one of its first three
/// arguments' basenames is `program`, or `program` with a script extension.
pub(super) fn is_command_for(command: &str, program: &str) -> bool {
    command.split_whitespace().take(3).any(|arg| {
        let path = Path::new(arg);
        let name = path.file_name().and_then(|name| name.to_str());
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        let script = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| SCRIPT_EXTENSIONS.contains(&ext));
        name == Some(program) || (script && stem == Some(program))
    })
}
