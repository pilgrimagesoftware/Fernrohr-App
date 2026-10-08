use super::{RenameError, SavedLayout, load_all, remove, rename, save, slugify};
use gpui_kit::component::dock::DockAreaState;
use std::fs;
use std::path::PathBuf;

fn temp_dir(label: &str) -> PathBuf {
    crate::util::test_paths::temp_path(label)
}

fn sample_layout(name: &str) -> SavedLayout {
    SavedLayout {
        version: 1,
        name: name.to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        contexts: vec!["dev".to_string()],
        dock: DockAreaState::default(),
        resource_panel_width: Some(240.0),
        window_width: 1024.0,
        window_height: 768.0,
    }
}

/// 1.1: a `SavedLayout` round-trips through `serde_json`.
#[test]
fn a_saved_layout_round_trips_through_json() {
    let layout = sample_layout("My Layout");
    let json = serde_json::to_string(&layout).unwrap();
    let parsed: SavedLayout = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, layout);
}

/// 1.2: two display names that differ only in spacing vs. punctuation derive
/// the same stem.
#[test]
fn names_that_differ_only_in_punctuation_slugify_alike() {
    assert_eq!(slugify("My Layout"), slugify("my layout"));
    assert_eq!(slugify("My Layout"), "my-layout");
}

/// 1.2: a name that is all punctuation slugifies to the placeholder stem.
#[test]
fn an_all_punctuation_name_uses_the_placeholder_stem() {
    assert_eq!(slugify("!!!"), crate::consts::SAVED_LAYOUT_PLACEHOLDER_STEM);
    assert_eq!(slugify("   "), crate::consts::SAVED_LAYOUT_PLACEHOLDER_STEM);
}

/// 1.3: saving creates `layouts/` on first use.
#[test]
fn saving_creates_the_directory_on_first_use() {
    let dir = temp_dir("saved-layouts-create-dir");
    assert!(!dir.exists());

    let path = save(&dir, &sample_layout("Foo")).unwrap();

    assert!(dir.is_dir());
    assert!(path.exists());
    let _ = fs::remove_dir_all(&dir);
}

/// 1.3: two different display names that slugify alike get distinct files, so
/// neither save overwrites the other.
#[test]
fn two_names_that_slugify_alike_get_distinct_files() {
    let dir = temp_dir("saved-layouts-slug-collision");
    fs::create_dir_all(&dir).unwrap();

    let first = save(&dir, &sample_layout("My Layout")).unwrap();
    let second = save(&dir, &sample_layout("my_layout")).unwrap();

    assert_ne!(first, second);
    let (layouts, unreadable) = load_all(&dir);
    assert!(unreadable.is_empty());
    let mut names: Vec<&str> = layouts.iter().map(|l| l.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["My Layout", "my_layout"]);
    let _ = fs::remove_dir_all(&dir);
}

/// 1.3: saving over the same name (even differing only by case) overwrites
/// the one file rather than creating a second.
#[test]
fn saving_over_the_same_name_overwrites_in_place() {
    let dir = temp_dir("saved-layouts-overwrite");
    fs::create_dir_all(&dir).unwrap();

    let first = save(&dir, &sample_layout("Foo")).unwrap();
    let mut updated = sample_layout("foo");
    updated.window_width = 1280.0;
    let second = save(&dir, &updated).unwrap();

    assert_eq!(first, second, "same derived name overwrites the one file");
    let (layouts, _) = load_all(&dir);
    assert_eq!(layouts.len(), 1);
    assert_eq!(layouts[0].window_width, 1280.0);
    let _ = fs::remove_dir_all(&dir);
}

/// 1.3: a leftover temp file from an interrupted write never touches the real
/// file of the same derived name - the invariant `write_atomic` relies on
/// (write to a temp file, `fs::rename` onto the final path) is that nothing
/// is visible at the final path until that rename, so simulating the crash
/// (writing the temp file by hand, never renaming it) must leave the
/// existing file exactly as it was.
#[test]
fn an_interrupted_write_never_touches_the_existing_file() {
    let dir = temp_dir("saved-layouts-interrupted");
    fs::create_dir_all(&dir).unwrap();
    let path = save(&dir, &sample_layout("Foo")).unwrap();
    let before = fs::read_to_string(&path).unwrap();

    // Simulate a save that wrote its temp file and crashed before the
    // rename onto `path` - `write_atomic` itself never ran for this write.
    let tmp_path = dir.join(".foo.json.tmp-crashed");
    fs::write(&tmp_path, "{ not valid json, mid-write").unwrap();

    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(
        before, after,
        "the leftover temp file never touched the real one"
    );
    assert!(tmp_path.exists());
    let _ = fs::remove_dir_all(&dir);
}

/// 1.4: a directory with two valid files and one corrupt file returns both
/// valid layouts and one `UnreadableLayout` naming the corrupt file; the
/// corrupt file's bytes are unchanged afterward.
#[test]
fn one_unreadable_file_does_not_affect_the_others() {
    let dir = temp_dir("saved-layouts-unreadable");
    fs::create_dir_all(&dir).unwrap();
    save(&dir, &sample_layout("Alpha")).unwrap();
    save(&dir, &sample_layout("Beta")).unwrap();
    let corrupt_path = dir.join("corrupt.json");
    fs::write(&corrupt_path, "not valid json {{{").unwrap();

    let (layouts, unreadable) = load_all(&dir);

    let mut names: Vec<&str> = layouts.iter().map(|l| l.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["Alpha", "Beta"]);
    assert_eq!(unreadable.len(), 1);
    assert_eq!(unreadable[0].filename, "corrupt.json");
    assert_eq!(
        fs::read_to_string(&corrupt_path).unwrap(),
        "not valid json {{{",
        "the unreadable file is left unchanged"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A missing `layouts/` directory loads as empty, not an error.
#[test]
fn a_missing_directory_loads_as_empty() {
    let dir = temp_dir("saved-layouts-missing");
    assert!(!dir.exists());
    let (layouts, unreadable) = load_all(&dir);
    assert!(layouts.is_empty());
    assert!(unreadable.is_empty());
}

/// 1.5: renaming to a name already in use (including a same-only-by-case
/// match) returns `NameTaken` and leaves both files unchanged.
#[test]
fn renaming_to_a_taken_name_is_rejected_case_insensitively() {
    let dir = temp_dir("saved-layouts-rename-taken");
    fs::create_dir_all(&dir).unwrap();
    save(&dir, &sample_layout("Alpha")).unwrap();
    save(&dir, &sample_layout("Beta")).unwrap();

    let result = rename(&dir, "Alpha", "BETA");

    assert!(matches!(result, Err(RenameError::NameTaken)));
    let (mut layouts, _) = load_all(&dir);
    layouts.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<&str> = layouts.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        ["Alpha", "Beta"],
        "both saved layouts keep their names"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// 1.5: renaming to a free name updates the file's `name` field and, when
/// the derived filename changed, leaves exactly one file (the new one) in
/// the directory.
#[test]
fn renaming_to_a_free_name_updates_the_file_and_filename() {
    let dir = temp_dir("saved-layouts-rename-free");
    fs::create_dir_all(&dir).unwrap();
    save(&dir, &sample_layout("Alpha")).unwrap();

    rename(&dir, "Alpha", "Gamma").unwrap();

    let (layouts, unreadable) = load_all(&dir);
    assert!(unreadable.is_empty());
    assert_eq!(layouts.len(), 1, "exactly one file remains");
    assert_eq!(layouts[0].name, "Gamma");
    assert!(!dir.join("alpha.json").exists());
    assert!(dir.join("gamma.json").exists());
    let _ = fs::remove_dir_all(&dir);
}

/// 1.5: renaming a layout to a case-variant of its own name is allowed (not
/// a collision with itself) and keeps it to one file.
#[test]
fn renaming_to_a_case_variant_of_itself_succeeds() {
    let dir = temp_dir("saved-layouts-rename-self-case");
    fs::create_dir_all(&dir).unwrap();
    save(&dir, &sample_layout("Alpha")).unwrap();

    rename(&dir, "Alpha", "ALPHA").unwrap();

    let (layouts, _) = load_all(&dir);
    assert_eq!(layouts.len(), 1);
    assert_eq!(layouts[0].name, "ALPHA");
    let _ = fs::remove_dir_all(&dir);
}

/// 1.5: renaming a name that identifies no saved layout reports `NotFound`.
#[test]
fn renaming_an_absent_name_is_not_found() {
    let dir = temp_dir("saved-layouts-rename-missing");
    fs::create_dir_all(&dir).unwrap();
    save(&dir, &sample_layout("Alpha")).unwrap();

    let result = rename(&dir, "Nope", "Gamma");

    assert!(matches!(result, Err(RenameError::NotFound)));
    let _ = fs::remove_dir_all(&dir);
}

/// 1.5: removing an absent name is a no-op.
#[test]
fn removing_an_absent_name_is_a_no_op() {
    let dir = temp_dir("saved-layouts-remove-missing");
    fs::create_dir_all(&dir).unwrap();
    save(&dir, &sample_layout("Alpha")).unwrap();

    remove(&dir, "Nope").unwrap();

    let (layouts, _) = load_all(&dir);
    assert_eq!(layouts.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

/// Removing a saved layout by name deletes exactly its file.
#[test]
fn removing_a_saved_layout_deletes_its_file() {
    let dir = temp_dir("saved-layouts-remove");
    fs::create_dir_all(&dir).unwrap();
    save(&dir, &sample_layout("Alpha")).unwrap();
    save(&dir, &sample_layout("Beta")).unwrap();

    remove(&dir, "Alpha").unwrap();

    let (layouts, _) = load_all(&dir);
    assert_eq!(layouts.len(), 1);
    assert_eq!(layouts[0].name, "Beta");
    let _ = fs::remove_dir_all(&dir);
}
