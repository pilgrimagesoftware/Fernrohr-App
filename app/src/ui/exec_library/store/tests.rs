//! The library's file across launches: a relaunch reads back what was saved,
//! in order, and a broken file is reported, refuses edits, and is never
//! touched.

use super::ExecLibraryStore;
use crate::config::exec_library::{Entry, EntryError, Target};
use gpui_kit::TestAppContext;

fn entry(name: &str) -> Entry {
    Entry {
        name: name.into(),
        command: "date".into(),
        container: None,
        target: Target::Any,
    }
}

fn names(cx: &gpui_kit::App) -> Vec<String> {
    ExecLibraryStore::get(cx)
        .entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

/// Spec "Restart": two saved entries come back after a relaunch, in order.
#[gpui_kit::test]
fn saved_entries_come_back_after_a_relaunch_in_order(cx: &mut TestAppContext) {
    let path = crate::util::test_paths::temp_path("exec-library-restart");
    cx.update(|cx| {
        ExecLibraryStore::init(path.clone(), cx);
        for name in ["Zeta", "Alpha"] {
            ExecLibraryStore::update(cx, |library| library.create(entry(name))).expect("saved");
        }
    });

    let relaunched = TestAppContext::single();
    relaunched.update(|cx| {
        ExecLibraryStore::init(path.clone(), cx);
        assert_eq!(names(cx), ["Zeta", "Alpha"]);
        assert!(ExecLibraryStore::broken(cx).is_none());
    });
    let _ = std::fs::remove_file(&path);
}

/// Spec "Broken file": invalid TOML is reported, the library is empty and
/// read-only, and the file's bytes are unchanged.
#[gpui_kit::test]
fn a_broken_file_is_reported_read_only_and_untouched(cx: &mut TestAppContext) {
    let path = crate::util::test_paths::temp_path("exec-library-broken");
    let text = "[[entries]]\nname = \"half an edit\ncommand = date\n";
    std::fs::write(&path, text).expect("written");
    cx.update(|cx| {
        ExecLibraryStore::init(path.clone(), cx);
        assert!(names(cx).is_empty());
        let reason = ExecLibraryStore::broken(cx).expect("reported");
        assert!(
            reason.contains("exec-library-broken"),
            "names the file: {reason}"
        );
        assert_eq!(
            ExecLibraryStore::update(cx, |library| library.create(entry("x"))),
            Err(EntryError::ReadOnly)
        );
    });
    assert_eq!(std::fs::read_to_string(&path).expect("still there"), text);
    let _ = std::fs::remove_file(&path);
}

/// A missing file is an empty library, and nothing is written until a change.
#[gpui_kit::test]
fn a_missing_file_is_an_empty_library_until_the_first_change(cx: &mut TestAppContext) {
    let path = crate::util::test_paths::temp_path("exec-library-missing");
    let _ = std::fs::remove_file(&path);
    cx.update(|cx| {
        ExecLibraryStore::init(path.clone(), cx);
        assert!(names(cx).is_empty());
    });
    assert!(!path.exists(), "not written yet");
    cx.update(|cx| {
        ExecLibraryStore::update(cx, |library| library.create(entry("x"))).expect("saved");
    });
    assert!(path.exists());
    let _ = std::fs::remove_file(&path);
}

/// A file that can't be read as text - not UTF-8 - is no empty library: it is
/// reported, read-only, and its bytes survive an attempted save.
#[gpui_kit::test]
fn an_unreadable_file_is_read_only_and_untouched(cx: &mut TestAppContext) {
    let path = crate::util::test_paths::temp_path("exec-library-not-utf8");
    let bytes = [b'n', b'a', b'm', b'e', 0xff, 0xfe, b'\n'];
    std::fs::write(&path, bytes).expect("written");
    cx.update(|cx| {
        ExecLibraryStore::init(path.clone(), cx);
        let reason = ExecLibraryStore::broken(cx).expect("reported");
        assert!(
            reason.contains("exec-library-not-utf8"),
            "names the file: {reason}"
        );
        assert_eq!(
            ExecLibraryStore::update(cx, |library| library.create(entry("x"))),
            Err(EntryError::ReadOnly)
        );
    });
    assert_eq!(std::fs::read(&path).expect("still there"), bytes);
    let _ = std::fs::remove_file(&path);
}
