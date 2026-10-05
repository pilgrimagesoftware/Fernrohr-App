//! `command-tunnels` 2.1: the login `PATH` is read between the markers, whatever an
//! rc file prints around it, and a shell that gives nothing falls back to the process
//! `PATH` plus the usual install directories.

use super::*;

fn marked(path: &str) -> String {
    format!("{MARKER}{path}{MARKER}")
}

#[test]
fn the_path_between_the_markers_is_used() {
    let output = format!(
        "Welcome back!\n{}\nlast login: today",
        marked("/a/bin:/b/bin")
    );
    let path = resolve(|| Some(output), "/usr/bin", None);
    assert_eq!(path, "/a/bin:/b/bin");
}

#[test]
fn no_output_falls_back_to_the_process_path_plus_install_dirs() {
    let home = Path::new("/home/someone");
    let path = resolve(|| None, "/usr/bin:/bin", Some(home));
    assert_eq!(
        path,
        "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin:/home/someone/.local/bin"
    );
}

#[test]
fn output_without_markers_or_an_empty_value_falls_back() {
    assert_eq!(
        resolve(|| Some("noise".into()), "/usr/bin", None),
        "/usr/bin:/opt/homebrew/bin:/usr/local/bin"
    );
    assert_eq!(
        resolve(|| Some(marked("  ")), "/usr/bin", None),
        "/usr/bin:/opt/homebrew/bin:/usr/local/bin"
    );
}

#[test]
fn the_fallback_skips_directories_already_on_the_path() {
    let path = fallback("/usr/local/bin:/usr/bin", None);
    assert_eq!(path, "/usr/local/bin:/usr/bin:/opt/homebrew/bin");
}
