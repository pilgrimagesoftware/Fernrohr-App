//! Scratch file paths for tests: unique to this test process and this call,
//! and empty when handed out.
//!
//! A name made from a counter alone is reused by every run, so a file an
//! earlier run left behind - a `keymap.toml` with a key rebound, a saved
//! workspace - is read back by a later test as if it were its own. The process
//! id keeps runs apart, the counter keeps calls apart, and any file already at
//! the path is removed first, so nothing stale is ever read.
//!
//! The file isn't removed afterwards: most tests hand the path to app state
//! (`util::shell::init`, a store) that outlives the test body, so a guard that
//! deleted on drop could pull the file out from under a live app. The
//! temporary directory's own cleanup takes them.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A path no other call in any run has been given, with no file at it.
/// `label` says which test made it, for anyone reading the temp directory.
pub(crate) fn temp_path(label: &str) -> PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "fernrohr-test-{label}-{}-{n}.toml",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

#[cfg(test)]
mod tests {
    use super::temp_path;
    use std::collections::HashSet;

    #[test]
    fn calls_never_collide() {
        let paths: HashSet<_> = (0..100).map(|_| temp_path("collide")).collect();
        assert_eq!(paths.len(), 100);
    }

    #[test]
    fn a_path_carries_this_process_and_starts_empty() {
        let path = temp_path("fresh");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.contains(&format!("-{}-", std::process::id())),
            "{name} names this process"
        );
        assert!(!path.exists(), "nothing is at a new path");
    }
}
