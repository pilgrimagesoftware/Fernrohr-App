//! Shared by the binary-level tests: the built `fernrohr`, run with a fresh,
//! empty home so it can neither touch the developer's own Fernrohr (its
//! tunnels, log, preferences or MCP socket) nor hide that it wrote something.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A directory that holds the test's `HOME` and XDG directories, none of which
/// exist until something creates them. Removed on drop.
pub struct FreshHome {
    root: PathBuf,
}

impl FreshHome {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "fernrohr-cli-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("create the test home");
        Self { root }
    }

    /// The environment that points `HOME` and the XDG directories inside this
    /// one.
    pub fn vars(&self) -> Vec<(&'static str, PathBuf)> {
        [
            ("HOME", "home"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_RUNTIME_DIR", "runtime"),
        ]
        .into_iter()
        .map(|(var, dir)| (var, self.root.join(dir)))
        .collect()
    }

    /// `fernrohr` with `args`, in this home.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fernrohr"));
        command.args(args).envs(self.vars());
        command
    }

    /// Every path created under this home, relative to it.
    // Each test crate compiles its own copy of this module, and not every one
    // asks what was created.
    #[allow(dead_code)]
    pub fn created(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        walk(&self.root, &self.root, &mut found);
        found
    }
}

#[allow(dead_code)] // Only `created` walks; see there.
fn walk(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        found.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        if path.is_dir() {
            walk(root, &path, found);
        }
    }
}

impl Drop for FreshHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
