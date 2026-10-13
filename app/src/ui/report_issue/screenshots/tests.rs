use std::path::PathBuf;

use super::{add, reveal_commands};
use crate::consts::REPORT_MAX_SCREENSHOTS;

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names
        .iter()
        .map(|name| PathBuf::from(format!("/shots/{name}")))
        .collect()
}

#[test]
fn images_are_kept_and_other_files_skipped() {
    let mut chosen = Vec::new();
    let skipped = add(
        &mut chosen,
        paths(&["a.png", "b.JPG", "c.jpeg", "d.gif", "notes.txt", "e"]),
    );
    assert_eq!(chosen, paths(&["a.png", "b.JPG", "c.jpeg", "d.gif"]));
    assert_eq!(skipped, 2);
}

#[test]
fn duplicates_are_ignored_without_counting_as_skipped() {
    let mut chosen = paths(&["a.png"]);
    let skipped = add(&mut chosen, paths(&["a.png", "b.png"]));
    assert_eq!(chosen, paths(&["a.png", "b.png"]));
    assert_eq!(skipped, 0);
}

#[test]
fn the_limit_holds_and_the_rest_are_skipped() {
    let mut chosen = Vec::new();
    let picked: Vec<PathBuf> = (0..REPORT_MAX_SCREENSHOTS + 2)
        .map(|n| PathBuf::from(format!("/shots/{n}.png")))
        .collect();
    let skipped = add(&mut chosen, picked);
    assert_eq!(chosen.len(), REPORT_MAX_SCREENSHOTS);
    assert_eq!(skipped, 2);
}

#[test]
fn files_are_revealed_one_by_one_on_macos_and_by_folder_elsewhere() {
    let shots = vec![
        PathBuf::from("/a/one.png"),
        PathBuf::from("/a/two.png"),
        PathBuf::from("/b/three.png"),
    ];
    let commands = reveal_commands(&shots);
    let described: Vec<(String, Vec<String>)> = commands
        .iter()
        .map(|command| {
            (
                command.get_program().to_string_lossy().into_owned(),
                command
                    .get_args()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect(),
            )
        })
        .collect();
    if cfg!(target_os = "macos") {
        assert_eq!(
            described,
            [
                ("open".into(), vec!["-R".into(), "/a/one.png".into()]),
                ("open".into(), vec!["-R".into(), "/a/two.png".into()]),
                ("open".into(), vec!["-R".into(), "/b/three.png".into()]),
            ]
        );
    } else {
        assert_eq!(
            described,
            [
                ("xdg-open".into(), vec!["/a".into()]),
                ("xdg-open".into(), vec!["/b".into()]),
            ]
        );
    }
}
