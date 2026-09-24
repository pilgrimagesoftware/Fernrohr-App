use crate::command::CommandRegistry;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// User-editable key overrides, keyed by command id. `preference_dir()/keymap.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeymapConfig {
    pub bindings: BTreeMap<String, String>,
}

/// Loads `keymap.toml`. On first run (no file), writes one containing every
/// registered command's default binding and returns that. An existing file
/// is never rewritten by this loader, however it parses - see [`resolve`]
/// for how an invalid entry falls back without touching the file.
pub fn load(path: &Path, registry: &CommandRegistry) -> KeymapConfig {
    match std::fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
        Err(_) => {
            let defaults = KeymapConfig {
                bindings: registry
                    .iter()
                    .map(|command| (command.id.to_string(), command.default_binding.to_string()))
                    .collect(),
            };
            let _ = crate::config::save(path, &defaults);
            defaults
        }
    }
}

/// The effective keystroke for `command`: `keymap`'s override, or the
/// command's default if there's no override, the override is empty (an
/// invalid entry), or it names a different command.
// ponytail: only catches an empty override, not a syntactically malformed
// but non-empty one (GPUI's own KeyBinding::new panics on those). Validate
// against GPUI's keystroke grammar here if a hand-edited keymap.toml with a
// typo'd-but-nonempty binding turns out to be a real problem.
pub fn resolve(command_id: &str, default_binding: &str, keymap: &KeymapConfig) -> String {
    match keymap.bindings.get(command_id) {
        Some(binding) if !binding.trim().is_empty() => binding.clone(),
        _ => default_binding.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{KeymapConfig, load, resolve};
    use crate::command::{Command, CommandRegistry};
    use gpui_kit::actions;
    use std::sync::atomic::{AtomicU64, Ordering};

    actions!(keymap_test, [TestAction]);

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("fernrohr-keymap-test-{n}.toml"))
    }

    fn registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        registry.register(Command {
            id: "test.command",
            title: "Test Command",
            default_binding: "cmd-t",
            context: None,
            action: Box::new(TestAction),
        });
        registry
    }

    #[test]
    fn first_run_writes_every_default_binding() {
        let path = temp_path();
        let registry = registry();

        let keymap = load(&path, &registry);

        assert_eq!(
            keymap.bindings.get("test.command"),
            Some(&"cmd-t".to_string())
        );
        let on_disk: KeymapConfig =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk, keymap);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn override_rebinds_after_restart() {
        let path = temp_path();
        let registry = registry();
        std::fs::write(&path, "[bindings]\n\"test.command\" = \"cmd-shift-t\"\n").unwrap();

        let keymap = load(&path, &registry);
        let command = registry.get("test.command").unwrap();
        let effective = resolve(command.id, command.default_binding, &keymap);

        assert_eq!(effective, "cmd-shift-t");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_entry_falls_back_to_default_and_leaves_file_untouched() {
        let path = temp_path();
        let registry = registry();
        let contents = "[bindings]\n\"test.command\" = \"\"\n";
        std::fs::write(&path, contents).unwrap();

        let keymap = load(&path, &registry);
        let command = registry.get("test.command").unwrap();
        let effective = resolve(command.id, command.default_binding, &keymap);

        assert_eq!(
            effective, "cmd-t",
            "empty override should fall back to default"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn entry_for_unknown_command_id_is_ignored() {
        let path = temp_path();
        let registry = registry();
        std::fs::write(&path, "[bindings]\n\"nonexistent.command\" = \"cmd-x\"\n").unwrap();

        let keymap = load(&path, &registry);
        let command = registry.get("test.command").unwrap();
        let effective = resolve(command.id, command.default_binding, &keymap);

        assert_eq!(effective, "cmd-t");
        let _ = std::fs::remove_file(&path);
    }
}
