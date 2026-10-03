//! The scratch paths shell tests get: never shared, and never holding a file
//! an earlier run left behind.

use super::{temp_tunnels_path, temp_workspace_path};
use crate::command::CommandRegistry;
use crate::util::shell::app::TOGGLE_PALETTE_COMMAND_ID;
use std::collections::HashSet;

#[test]
fn two_calls_never_collide() {
    let paths: HashSet<_> = (0..50)
        .flat_map(|_| [temp_workspace_path(), temp_tunnels_path()])
        .collect();
    assert_eq!(paths.len(), 100);
}

/// The old helper named its files `fernrohr-shell-test-{n}.toml` from a counter
/// alone, so every run reused them. A stale `keymap.toml` at such a name - one
/// rebinding a command - must not be what a new test's keymap loads.
#[test]
fn a_stale_file_at_an_old_style_path_is_not_read() {
    let stale = format!("[bindings]\n\"{TOGGLE_PALETTE_COMMAND_ID}\" = \"cmd-shift-m\"\n");
    let old: Vec<_> = (0..64)
        .map(|n| std::env::temp_dir().join(format!("fernrohr-shell-test-{n}.toml")))
        .collect();
    for path in &old {
        std::fs::write(path, &stale).unwrap();
    }

    let path = temp_workspace_path();
    assert!(!old.contains(&path), "a new-style name: {path:?}");
    assert!(!path.exists(), "nothing is waiting at it");
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    let keymap = crate::keymap::load(&path, &registry);
    assert_ne!(
        keymap
            .bindings
            .get(TOGGLE_PALETTE_COMMAND_ID)
            .map(String::as_str),
        Some("cmd-shift-m"),
        "the stale rebinding isn't read"
    );

    for path in old {
        let _ = std::fs::remove_file(path);
    }
}
