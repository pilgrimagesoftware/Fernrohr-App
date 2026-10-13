//! Screenshots a report names: which picked files are kept, and how they're
//! shown once the report is handed off. `gh issue create` can't upload files
//! and the browser page can't be prefilled with them, so the user drags them
//! onto the issue; Fernrohr only puts them where that's one step away.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::consts::REPORT_MAX_SCREENSHOTS;

/// The image types GitHub shows inline, by extension.
const IMAGE_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "gif"];

/// Adds the images in `picked` to `chosen`, up to the limit, ignoring ones
/// already chosen. Returns how many were skipped: not images, or over the limit.
pub(super) fn add(chosen: &mut Vec<PathBuf>, picked: Vec<PathBuf>) -> usize {
    let mut skipped = 0;
    for path in picked {
        if chosen.contains(&path) {
            continue;
        }
        if !is_image(&path) || chosen.len() >= REPORT_MAX_SCREENSHOTS {
            skipped += 1;
            continue;
        }
        chosen.push(path);
    }
    skipped
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

/// The commands that show `paths`: each file revealed in Finder on macOS, and
/// each distinct folder opened on Linux, where no file manager selection is
/// portable.
pub(super) fn reveal_commands(paths: &[PathBuf]) -> Vec<Command> {
    if cfg!(target_os = "macos") {
        paths
            .iter()
            .map(|path| {
                let mut command = Command::new("open");
                command.arg("-R").arg(path);
                command
            })
            .collect()
    } else {
        let mut folders: Vec<&Path> = Vec::new();
        for folder in paths.iter().filter_map(|path| path.parent()) {
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        folders
            .into_iter()
            .map(|folder| {
                let mut command = Command::new("xdg-open");
                command.arg(folder);
                command
            })
            .collect()
    }
}

/// Shows `paths` off the main thread. A file manager that fails to open is
/// logged, not reported: the issue is already filed, and its body names them.
pub(super) fn reveal(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for mut command in reveal_commands(&paths) {
            if let Err(error) = command.status() {
                log::warn!("couldn't show the report's screenshots: {error}");
            }
        }
    });
}

#[cfg(test)]
mod tests;
