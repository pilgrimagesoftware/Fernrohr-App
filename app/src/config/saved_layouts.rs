//! Saved panel layouts: one file per layout under `state_dir()/layouts/`
//! (`saved-panel-layouts` design.md D2). This module owns the on-disk shape
//! (`SavedLayout`), filename derivation, and plain file I/O (save, load,
//! rename, remove) - it knows nothing about a live window, a dock, or a
//! panel's content key; `util::shell::saved_layouts` (a later section) is
//! where a live window turns into one of these and back.
//!
//! Every function here is pure over `dir: &Path` rather than reading
//! `state_dir()` itself, so tests exercise it against a temp directory with
//! no interaction between runs.
//!
//! `save` and `load_all` are wired for real by the Save Panel Layout command
//! (`util::shell::saved_layouts`, tasks section 2): `save` writes a new or
//! overwritten layout, and `load_all` is how its naming dialog checks a typed
//! name against what's already saved. `rename` and `remove` are wired by the
//! saved layouts picker (`ui::picker::saved_layouts`, tasks section 3); the
//! Settings Layouts section (tasks section 6) calls `remove` too.

use gpui_kit::component::dock::DockAreaState;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// One saved layout's on-disk shape - the whole content of one file under
/// `layouts/` (design.md D2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedLayout {
    /// Schema version; bumped on a breaking shape change.
    pub version: u32,
    /// The display name the user gave it - source of truth, independent of
    /// the file's own derived, sanitized name.
    pub name: String,
    /// RFC 3339, set once when the layout is first saved.
    pub created_at: String,
    /// RFC 3339, bumped on overwrite and rename.
    pub updated_at: String,
    /// Same shape as `WindowLayout.contexts`.
    pub contexts: Vec<String>,
    /// The same dock-dump type `dock-layouts.json` already stores.
    pub dock: DockAreaState,
    /// Same meaning as `WindowLayout.resource_panel_width`: `None` is hidden,
    /// `Some(width)` is visible at that width.
    pub resource_panel_width: Option<f32>,
    pub window_width: f32,
    pub window_height: f32,
}

/// A file under `layouts/` that failed to read or parse, reported by its
/// filename - it has no display name to offer instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableLayout {
    pub filename: String,
}

/// Why [`rename`] couldn't carry out a rename as asked.
#[derive(Debug)]
pub enum RenameError {
    /// `new_name` already identifies a different saved layout (compared
    /// case-insensitively). Renaming to a case-variant of the same layout's
    /// own name is not this - it succeeds.
    NameTaken,
    /// `old_name` doesn't identify any saved layout in `dir`.
    NotFound,
    /// The new or old file couldn't be written, read, or removed.
    Io(io::Error),
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenameError::NameTaken => f.write_str("Another saved layout already has that name."),
            RenameError::NotFound => f.write_str("There is no saved layout with that name."),
            RenameError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for RenameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RenameError::Io(err) => Some(err),
            RenameError::NameTaken | RenameError::NotFound => None,
        }
    }
}

impl From<io::Error> for RenameError {
    fn from(err: io::Error) -> Self {
        RenameError::Io(err)
    }
}

/// A display name, lowercased, with every run of characters outside
/// `[a-z0-9]` collapsed to a single `-` and leading/trailing `-` trimmed; a
/// name that collapses to nothing (all punctuation, or non-ASCII with no
/// ASCII fallback) uses [`crate::consts::SAVED_LAYOUT_PLACEHOLDER_STEM`]
/// instead (design.md D2).
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_dash = false;
    for ch in name.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            slug.push(lower);
            last_was_dash = false;
        } else if !slug.is_empty() && !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        crate::consts::SAVED_LAYOUT_PLACEHOLDER_STEM.to_string()
    } else {
        slug
    }
}

/// `path`'s own `SavedLayout.name`, or `None` if it doesn't exist or doesn't
/// parse. Reads just enough to answer "whose file is this", without caring
/// whether the rest of its shape is still valid.
fn saved_name_at(path: &Path) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    value.get("name")?.as_str().map(str::to_string)
}

/// The path to save a layout named `display_name` (already slugified to
/// `stem`) under `dir`: the first of `<stem>.json`, `<stem>-2.json`, ... that
/// is free, already named `display_name` case-insensitively (an overwrite),
/// or is `reuse` - the file a rename is moving away from, which never counts
/// as a collision with itself (design.md D2's filename derivation).
fn target_path(dir: &Path, stem: &str, display_name: &str, reuse: Option<&Path>) -> PathBuf {
    let mut suffix = 1u32;
    loop {
        let candidate = if suffix == 1 {
            dir.join(format!("{stem}.json"))
        } else {
            dir.join(format!("{stem}-{suffix}.json"))
        };
        if reuse == Some(candidate.as_path()) {
            return candidate;
        }
        match saved_name_at(&candidate) {
            None => return candidate,
            Some(existing) if existing.eq_ignore_ascii_case(display_name) => return candidate,
            Some(_) => suffix += 1,
        }
    }
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `layout` to `path` atomically: a temp file in the same directory,
/// then `fs::rename`d onto `path` - a same-filesystem rename is atomic on
/// both macOS and Linux, so an interruption leaves either `path`'s previous
/// content or its new content, never a partial file (design.md D2).
fn write_atomic(path: &Path, layout: &SavedLayout) -> io::Result<()> {
    let contents =
        serde_json::to_string_pretty(layout).expect("a saved layout must serialize to JSON");
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("saved-layout.json");
    let tmp_name = format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let tmp_path = dir.join(tmp_name);
    fs::write(&tmp_path, contents)?;
    fs::rename(&tmp_path, path)
}

/// Saves `layout` under `dir` (created with `fs::create_dir_all` if missing),
/// returning the file it was written to. A filename collision with a
/// *different* saved name gets a numeric suffix (`-2`, `-3`, ...); a
/// collision with the *same* name (compared case-insensitively) overwrites
/// that file in place (design.md D2).
pub fn save(dir: &Path, layout: &SavedLayout) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stem = slugify(&layout.name);
    let path = target_path(dir, &stem, &layout.name, None);
    write_atomic(&path, layout)?;
    Ok(path)
}

/// Every `*.json` file directly under `dir`, each parsed if possible. An
/// unreadable `dir` (e.g. it doesn't exist yet) yields no entries, not an
/// error - `layouts/` is only created on first save.
fn entries(dir: &Path) -> Vec<(PathBuf, Option<SavedLayout>)> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .map(|path| {
            let layout = fs::read_to_string(&path)
                .ok()
                .and_then(|contents| serde_json::from_str(&contents).ok());
            (path, layout)
        })
        .collect()
}

/// Every saved layout under `dir`, sorted by name case-insensitively for a
/// stable listing, alongside every file that failed to read or parse -
/// reported by filename, left unmodified on disk (design.md D2's "reading
/// the collection"). A missing `dir` yields two empty lists, not an error.
pub fn load_all(dir: &Path) -> (Vec<SavedLayout>, Vec<UnreadableLayout>) {
    let mut layouts = Vec::new();
    let mut unreadable = Vec::new();
    for (path, parsed) in entries(dir) {
        match parsed {
            Some(layout) => layouts.push(layout),
            None => unreadable.push(UnreadableLayout {
                filename: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            }),
        }
    }
    layouts.sort_by_key(|layout| layout.name.to_lowercase());
    (layouts, unreadable)
}

/// Renames the saved layout named `old_name` to `new_name`: rejects a
/// case-insensitive collision with a different saved layout, otherwise
/// re-derives the filename from `new_name` the same way `save` does. When the
/// derived filename changes, the new file is written before the old one is
/// removed, so an interruption leaves the old file as the recoverable copy
/// (design.md D2).
pub fn rename(dir: &Path, old_name: &str, new_name: &str) -> Result<(), RenameError> {
    let all = entries(dir);
    let Some((old_path, mut layout)) = all.iter().find_map(|(path, parsed)| {
        let layout = parsed.as_ref()?;
        (layout.name == old_name).then(|| (path.clone(), layout.clone()))
    }) else {
        return Err(RenameError::NotFound);
    };

    let collides = all.iter().any(|(path, parsed)| {
        path != &old_path
            && parsed
                .as_ref()
                .is_some_and(|other| other.name.eq_ignore_ascii_case(new_name))
    });
    if collides {
        return Err(RenameError::NameTaken);
    }

    layout.name = new_name.to_string();
    layout.updated_at = jiff::Timestamp::now().to_string();

    let new_stem = slugify(new_name);
    let new_path = target_path(dir, &new_stem, new_name, Some(&old_path));
    write_atomic(&new_path, &layout)?;
    if new_path != old_path {
        fs::remove_file(&old_path)?;
    }
    Ok(())
}

/// Removes the saved layout named `name`, if one exists; an absent name is a
/// no-op, not an error.
pub fn remove(dir: &Path, name: &str) -> io::Result<()> {
    let found = entries(dir)
        .into_iter()
        .find(|(_, parsed)| parsed.as_ref().is_some_and(|layout| layout.name == name));
    match found {
        Some((path, _)) => fs::remove_file(path),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
