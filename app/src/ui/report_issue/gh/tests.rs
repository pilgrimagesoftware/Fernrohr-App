//! `gh` faked by a script on a scratch `PATH`: what Report asks of it, and how
//! its answers read.

use super::{REPO, create, ready};
use std::path::{Path, PathBuf};

/// A directory holding an executable `gh`, its script built from that
/// directory, and a `PATH` that finds it first.
pub(in crate::ui::report_issue) fn fake_gh(
    script: impl FnOnce(&Path) -> String,
) -> (PathBuf, String) {
    let dir = crate::util::test_paths::temp_path("gh");
    std::fs::create_dir_all(&dir).unwrap();
    let gh = dir.join("gh");
    std::fs::write(&gh, format!("#!/bin/sh\n{}\n", script(&dir))).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!("{}:/usr/bin:/bin", dir.display());
    (dir, path)
}

#[test]
fn gh_signed_in_is_ready_and_missing_or_signed_out_is_not() {
    let (_, signed_in) = fake_gh(|_| "exit 0".into());
    assert!(ready(&signed_in));
    let (_, signed_out) = fake_gh(|_| "exit 1".into());
    assert!(!ready(&signed_out));
    assert!(!ready("/nonexistent-dir"), "no gh on the PATH");
}

#[test]
fn create_files_against_the_app_repo_with_the_body_on_stdin() {
    let (dir, path) = fake_gh(|dir| {
        let dir = dir.display();
        format!(
            "echo \"$@\" > '{dir}/args'\ncat > '{dir}/body'\n\
             echo 'Creating issue in {REPO}'\necho 'https://github.com/{REPO}/issues/170'"
        )
    });

    let url = create(
        &path,
        "Pods panel crashes",
        "Steps: \"open\" it.\n\n---\nBuild x",
    );
    assert_eq!(url, Ok(format!("https://github.com/{REPO}/issues/170")));
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    assert_eq!(
        read("args").trim(),
        format!("issue create --repo {REPO} --title Pods panel crashes --body-file -")
    );
    assert_eq!(read("body"), "Steps: \"open\" it.\n\n---\nBuild x");
}

#[test]
fn a_failing_gh_says_why() {
    let (_, path) =
        fake_gh(|_| "cat > /dev/null\necho 'HTTP 401: Bad credentials' >&2\nexit 1".into());
    assert_eq!(
        create(&path, "t", "b"),
        Err("HTTP 401: Bad credentials".to_string())
    );
}
