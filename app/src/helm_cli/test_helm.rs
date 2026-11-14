//! A fake `helm` for tests (`helm-releases` D9): a shell script standing in
//! for the binary, so finding, versioning and running it need no real Helm.

use std::path::{Path, PathBuf};

/// A fresh, empty directory for one test.
pub(crate) fn temp_dir(label: &str) -> PathBuf {
    let dir = crate::util::test_paths::temp_path(&format!("helm-{label}"));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    dir
}

/// Writes an executable `name` in `dir` that runs `body` under `/bin/sh`,
/// and returns its path.
pub(crate) fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("a script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// A `helm` in `dir` reporting `version` to `helm version --short`, and a
/// PATH that finds it first.
pub(super) fn helm_reporting(dir: &Path, version: &str) -> (PathBuf, String) {
    let helm = script(dir, "helm", &format!("echo '{version}'"));
    (helm, format!("{}:/usr/bin:/bin", dir.display()))
}

/// `attempt` again while it fails with "Text file busy" (ETXTBSY), for up
/// to about a second. Tests run in parallel threads, and a process another
/// thread forks while `script` still has a fake `helm` open for writing
/// inherits that descriptor until it execs; Linux refuses to exec a file
/// anything holds open for writing. A real `helm` isn't freshly written, so
/// only tests retry.
pub(super) async fn past_busy<T, F>(mut attempt: impl FnMut() -> F) -> std::io::Result<T>
where
    F: Future<Output = std::io::Result<T>>,
{
    const ATTEMPTS: u32 = 100;
    let mut tries = 1;
    loop {
        match attempt().await {
            Err(error)
                if error.kind() == std::io::ErrorKind::ExecutableFileBusy && tries < ATTEMPTS =>
            {
                tries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            result => return result,
        }
    }
}
