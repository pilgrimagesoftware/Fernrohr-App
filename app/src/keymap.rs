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
/// command's default when there is no entry for it. An entry that is present
/// but empty means *no key* - how the keybindings editor's Remove is stored -
/// and resolves to `""`, which [`bindings`] skips like a palette-only command.
// An override GPUI can't parse still falls back to the default, in [`bindings`],
// which builds keys with the fallible `KeyBinding::load` rather than the
// panicking `KeyBinding::new`.
pub fn resolve(command_id: &str, default_binding: &str, keymap: &KeymapConfig) -> String {
    match keymap.bindings.get(command_id) {
        Some(binding) => binding.trim().to_string(),
        None => default_binding.to_string(),
    }
}

/// A key binding for every registered command: its `keymap` override, else its
/// default, in its own `KeyContext`. Built from the registry itself, so a registered
/// command can never end up with a menu item and palette entry but no key - the
/// failure a hand-maintained list of bindings allowed. An override that doesn't parse
/// falls back to the default; a command whose default doesn't parse is skipped.
pub fn bindings(
    registry: &CommandRegistry,
    keymap: &KeymapConfig,
    mapper: &dyn gpui_kit::PlatformKeyboardMapper,
) -> Vec<gpui_kit::KeyBinding> {
    registry
        .iter()
        .filter_map(|command| {
            let load =
                |keys: &str| load_binding(command, keys, command.action.boxed_clone(), mapper);
            let chosen = resolve(command.id, command.default_binding, keymap);
            // No key: a palette-only command (About, Zoom) with no override, or
            // one whose key the user removed. Nothing to bind or warn about.
            if chosen.is_empty() {
                return None;
            }
            load(&chosen).or_else(|| {
                log::warn!("keymap.toml: invalid binding {chosen:?} for {}", command.id);
                load(command.default_binding)
            })
        })
        .collect()
}

/// `keys` bound to `action` in `command`'s context - the command's own action,
/// or an `Unbind` of it. `None` when `keys` doesn't parse.
fn load_binding(
    command: &crate::command::Command,
    keys: &str,
    action: Box<dyn gpui_kit::Action>,
    mapper: &dyn gpui_kit::PlatformKeyboardMapper,
) -> Option<gpui_kit::KeyBinding> {
    let context = command.context.map(|context| {
        std::rc::Rc::new(
            gpui_kit::KeyBindingContextPredicate::parse(context)
                .expect("a registered command's context parses"),
        )
    });
    gpui_kit::KeyBinding::load(keys, action, context, false, None, mapper).ok()
}

mod completions;
mod conflicts;
mod live;

// UNWIRED(#131): the status bar's chord indicator is its caller, in the next
// section of `pending-chord-indicator`.
#[allow(unused_imports)]
pub use completions::{Completion, completions};
pub use conflicts::{Conflicts, PrefixKind, conflicts, lacks_modifier};
#[cfg(test)]
pub use conflicts::{Prefix, Shadow};
pub use live::{Edit, LiveKeymap, apply};

#[cfg(test)]
mod tests;
