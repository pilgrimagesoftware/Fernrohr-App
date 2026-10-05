//! Which scope each position means, and that the new keys collide with no
//! command the app registers (`keymap::conflicts`, 6.3).

use super::{JUMPS, scope_for};
use crate::command::CommandRegistry;
use crate::keymap::KeymapConfig;
use crate::keymap::conflicts;

fn names() -> Vec<String> {
    ["alpha", "beta", "gamma"].map(String::from).to_vec()
}

#[test]
fn a_position_picks_that_namespace() {
    assert_eq!(scope_for(&names(), 1), Some(vec!["alpha".to_string()]));
    assert_eq!(scope_for(&names(), 3), Some(vec!["gamma".to_string()]));
}

#[test]
fn zero_is_all_namespaces() {
    assert_eq!(scope_for(&names(), 0), Some(Vec::new()));
}

#[test]
fn a_position_past_the_end_changes_nothing() {
    assert_eq!(scope_for(&names(), 4), None);
    assert_eq!(scope_for(&[], 1), None);
}

/// 6.3: none of the quick-jump keys collides with another command the app
/// registers - neither in the same context nor by shadowing a global one.
#[test]
fn the_quick_jump_keys_collide_with_nothing() {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    for (id, _, keys) in JUMPS {
        assert!(registry.get(id).is_some(), "{id} is registered app-wide");
        let found = conflicts(&registry, &KeymapConfig::default(), id, keys);
        assert!(
            found.same_scope.is_empty() && found.shadows.is_empty(),
            "{id} on {keys} collides: {:?} / {} shadows",
            found.same_scope,
            found.shadows.len()
        );
    }
}
