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
///
/// Shorter keys come first. GPUI keeps a chord pending against a complete
/// shorter binding only when the chord was bound after it, so a key that is a
/// command on its own and also starts a chord (`cmd-k` against `cmd-k w`)
/// waits for the next key, as the Shortcut timeout specifies, rather than
/// running at once and making the chord unreachable.
///
/// Within each length, the user's `keymap.toml` overrides come after the
/// defaults: GPUI's later binding wins, so an override onto a key another
/// command has by default beats that default, whichever command was
/// registered first - as a live rebind, appended last, already does. The sort
/// is stable, so otherwise registration order is kept.
pub fn bindings(
    registry: &CommandRegistry,
    keymap: &KeymapConfig,
    mapper: &dyn gpui_kit::PlatformKeyboardMapper,
) -> Vec<gpui_kit::KeyBinding> {
    let mut bindings = registry
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
            let overridden = keymap.bindings.contains_key(command.id);
            match load(&chosen) {
                Some(binding) => Some((binding, overridden)),
                None => {
                    log::warn!("keymap.toml: invalid binding {chosen:?} for {}", command.id);
                    load(command.default_binding).map(|binding| (binding, false))
                }
            }
        })
        .collect::<Vec<_>>();
    bindings.sort_by_key(|(binding, overridden)| (binding.keystrokes().len(), *overridden));
    bindings.into_iter().map(|(binding, _)| binding).collect()
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

pub use completions::{Completion, completions};
pub use conflicts::{Conflicts, PrefixKind, conflicts, lacks_modifier};
#[cfg(test)]
pub use conflicts::{Prefix, Shadow};
pub use live::{Edit, LiveKeymap, apply};

#[cfg(test)]
mod tests;
