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
