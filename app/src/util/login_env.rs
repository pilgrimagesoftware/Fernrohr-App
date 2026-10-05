//! The user's login-shell `PATH`, for the commands a command tunnel runs.
//!
//! Launched from the Dock or a desktop launcher, the app inherits a minimal `PATH`
//! that rarely includes where a vendor CLI like `gcloud` was installed - that lives in
//! the user's shell startup files. So, once per run and only when first asked, this
//! runs the login shell (`$SHELL -lic`) to print its `PATH` between markers - the
//! markers let stray output from rc files be stripped - with a timeout, falling back
//! to the process `PATH` plus the usual install directories.
//!
//! Only the command tunnel's child gets this `PATH`; the app's own environment is
//! left alone, so exec-auth plugins behave as before.

use std::path::Path;
use std::sync::OnceLock;

const MARKER: &str = "__FERNROHR_LOGIN_PATH__";

/// Directories added to the process `PATH` when the login shell can't be asked.
const FALLBACK_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// The login shell's `PATH`, resolved on first call and cached for the run. Blocks
/// for up to [`crate::consts::LOGIN_SHELL_TIMEOUT`] the first time, so call it off
/// the main thread.
pub(crate) fn login_path() -> &'static str {
    static LOGIN_PATH: OnceLock<String> = OnceLock::new();
    LOGIN_PATH.get_or_init(|| {
        let process_path = std::env::var("PATH").unwrap_or_default();
        resolve(run_login_shell, &process_path, dirs::home_dir().as_deref())
    })
}

/// The login shell's `PATH` from `runner`'s output, or the fallback when it gives
/// none.
fn resolve(
    runner: impl FnOnce() -> Option<String>,
    process_path: &str,
    home: Option<&Path>,
) -> String {
    runner()
        .and_then(|output| parse(&output))
        .unwrap_or_else(|| fallback(process_path, home))
}

/// The value between the first two markers, if there is a non-empty one.
fn parse(output: &str) -> Option<String> {
    let (_, rest) = output.split_once(MARKER)?;
    let (path, _) = rest.split_once(MARKER)?;
    let path = path.trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// `process_path` with the usual install directories and `~/.local/bin` appended,
/// skipping any already on it.
fn fallback(process_path: &str, home: Option<&Path>) -> String {
    let mut dirs: Vec<String> = process_path
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(str::to_string)
        .collect();
    let local_bin = home.map(|home| home.join(".local/bin").display().to_string());
    for extra in FALLBACK_DIRS
        .iter()
        .map(|dir| dir.to_string())
        .chain(local_bin)
    {
        if !dirs.contains(&extra) {
            dirs.push(extra);
        }
    }
    dirs.join(":")
}

/// Runs `$SHELL -lic` to print its `PATH` between markers, killing it after
/// [`crate::consts::LOGIN_SHELL_TIMEOUT`]. `None` on any failure.
#[cfg(unix)]
fn run_login_shell() -> Option<String> {
    use std::io::Read as _;
    use std::process::{Command, Stdio};

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let script = format!("printf '{MARKER}%s{MARKER}' \"$PATH\"");
    let mut child = Command::new(shell)
        .args(["-lic", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Read on a thread, so a chatty rc file filling the pipe can't stall the wait.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut output = String::new();
        let _ = stdout.read_to_string(&mut output);
        output
    });
    let deadline = std::time::Instant::now() + crate::consts::LOGIN_SHELL_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    reader.join().ok()
}

#[cfg(not(unix))]
fn run_login_shell() -> Option<String> {
    None
}

#[cfg(test)]
mod tests;
