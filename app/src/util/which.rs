//! Finding a program the way a shell would: as given when it names a path,
//! else the first executable of that name on a `PATH` - the login shell's,
//! for the command tunnels and `helm` (`helm-releases` D8).

use std::path::{Path, PathBuf};

/// `program` as a path to run: as given when it names a path, otherwise the first
/// executable of that name on `path`.
pub(crate) fn find_program(program: &str, path: &str) -> Option<PathBuf> {
    if program.contains('/') {
        return Some(PathBuf::from(program));
    }
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(program))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
pub(crate) fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
pub(crate) fn is_executable(path: &Path) -> bool {
    path.is_file()
}
