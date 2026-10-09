use super::*;

const EXE: &str = "/Applications/Fernrohr.app/Contents/MacOS/fernrohr";

fn harness(id: &str) -> Harness {
    *HARNESSES
        .iter()
        .find(|harness| harness.id == id)
        .expect("a known harness")
}

#[test]
fn each_harness_registers_this_executable_at_user_scope() {
    let exe = Path::new(EXE);
    let quoted = format!("'{EXE}'");
    assert_eq!(
        harness("claude-code").command(exe),
        format!("claude mcp add --scope user fernrohr -- {quoted} mcp")
    );
    assert_eq!(
        harness("codex").command(exe),
        format!("codex mcp add fernrohr -- {quoted} mcp")
    );
    assert_eq!(
        harness("gemini-cli").command(exe),
        format!("gemini mcp add --scope user fernrohr {quoted} mcp")
    );
    assert_eq!(
        harness("opencode").command(exe),
        format!("opencode mcp add fernrohr --global -- {quoted} mcp")
    );
}

#[test]
fn the_harness_table_has_unique_ids_and_names() {
    let mut ids: Vec<_> = HARNESSES.iter().map(|harness| harness.id).collect();
    ids.dedup();
    assert_eq!(ids.len(), HARNESSES.len());
    assert!(
        HARNESSES
            .iter()
            .any(|harness| harness.id == SNIPPET_HARNESS)
    );
}

#[test]
fn opencodes_snippet_is_a_local_command_entry() {
    let snippet: serde_json::Value =
        serde_json::from_str(&opencode_snippet(Path::new(EXE))).unwrap();
    assert_eq!(
        snippet,
        serde_json::json!({"type": "local", "command": [EXE, "mcp"]})
    );
}

#[test]
fn paths_with_spaces_and_quotes_stay_one_shell_word() {
    assert_eq!(
        shell_quote("/opt/My Apps/fernrohr"),
        "'/opt/My Apps/fernrohr'"
    );
    assert_eq!(
        shell_quote("/home/o'brien/bin/fernrohr"),
        r"'/home/o'\''brien/bin/fernrohr'"
    );
    assert_eq!(shell_quote("/a/$HOME/`x`"), "'/a/$HOME/`x`'");
    let command = harness("codex").command(Path::new("/home/o'brien/My Apps/fernrohr"));
    assert_eq!(
        command,
        r"codex mcp add fernrohr -- '/home/o'\''brien/My Apps/fernrohr' mcp"
    );
}

#[test]
fn a_quoted_path_survives_a_real_shell() {
    let tricky = "/tmp/a b/it's \"$HOME\" `x`;rm";
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("printf %s {}", shell_quote(tricky)))
        .output()
        .expect("sh runs");
    assert_eq!(String::from_utf8_lossy(&output.stdout), tricky);
}

#[test]
fn translocated_and_build_tree_paths_are_unstable() {
    for (path, expected) in [
        (
            "/private/var/folders/ab/T/AppTranslocation/1A2B/d/Fernrohr.app/Contents/MacOS/fernrohr",
            Some(Unstable::Translocated),
        ),
        (
            "/home/me/src/Fernrohr-App/target/debug/fernrohr",
            Some(Unstable::BuildTree),
        ),
        (
            "/home/me/src/app/target/release/fernrohr",
            Some(Unstable::BuildTree),
        ),
        (
            "/home/me/src/app/target/aarch64-apple-darwin/release/fernrohr",
            Some(Unstable::BuildTree),
        ),
        (EXE, None),
        ("/usr/bin/fernrohr", None),
        // A directory merely named `target` isn't a build tree.
        ("/opt/target/bin/fernrohr", None),
    ] {
        assert_eq!(unstable(Path::new(path)), expected, "{path}");
    }
}

#[test]
fn an_appimage_registers_the_image_not_its_mount() {
    let setup = AgentSetup::for_executable(
        Some(PathBuf::from("/tmp/.mount_FernroXYZ/usr/bin/fernrohr")),
        Some(PathBuf::from("/home/me/Apps/Fernrohr.AppImage")),
    );
    assert_eq!(
        setup,
        AgentSetup::Ready {
            exe: PathBuf::from("/home/me/Apps/Fernrohr.AppImage")
        }
    );
}

#[test]
fn setup_follows_the_executables_stability() {
    assert_eq!(
        AgentSetup::for_executable(Some(PathBuf::from(EXE)), None),
        AgentSetup::Ready {
            exe: PathBuf::from(EXE)
        }
    );
    assert!(matches!(
        AgentSetup::for_executable(Some(PathBuf::from("/x/target/debug/fernrohr")), None),
        AgentSetup::Unstable {
            why: Unstable::BuildTree,
            ..
        }
    ));
    assert_eq!(
        AgentSetup::for_executable(None, None),
        AgentSetup::Unavailable
    );
}
