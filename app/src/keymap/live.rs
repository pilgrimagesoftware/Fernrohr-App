//! The keymap as the running app holds it, and editing it live: an edit is
//! saved to `keymap.toml` and re-binds just the one command.
//!
//! Re-binding never clears the app's keymap - that would also drop the
//! bindings no command owns (gpui-component's text input and lists, the
//! Resource panel's arrow keys). Instead an edit appends an `Unbind` of the
//! command's old key and a binding for its new one; later bindings win in
//! GPUI's keymap, and key hints (`Kbd`) honour `Unbind`, so both the key and
//! its hints follow at once.

use super::{KeymapConfig, load_binding, resolve};
use crate::command::{Command, CommandRegistry};
use gpui_kit::{App, Global, PlatformKeyboardMapper, Unbind};
use std::path::PathBuf;

/// The loaded `keymap.toml` and where it lives, kept for the app's lifetime
/// so the keybindings editor can read and change it.
pub struct LiveKeymap {
    path: PathBuf,
    config: KeymapConfig,
}

impl Global for LiveKeymap {}

impl LiveKeymap {
    pub fn new(path: PathBuf, config: KeymapConfig) -> Self {
        Self { path, config }
    }

    pub fn config(&self) -> &KeymapConfig {
        &self.config
    }
}

/// A change to one command's key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Bind this keystroke (or chord) instead.
    Set(String),
    /// Back to the command's default: its entry is removed.
    Reset,
    /// No key at all: stored as an empty entry (see [`super::resolve`]).
    Remove,
}

/// The key `command` is bound to under `config`, as [`super::bindings`]
/// chooses it: the override, or the default when the override doesn't parse.
/// `None` for no key.
fn bound_keys(
    command: &Command,
    config: &KeymapConfig,
    mapper: &dyn PlatformKeyboardMapper,
) -> Option<String> {
    let chosen = resolve(command.id, command.default_binding, config);
    if chosen.is_empty() {
        return None;
    }
    let parses =
        |keys: &str| load_binding(command, keys, command.action.boxed_clone(), mapper).is_some();
    if parses(&chosen) {
        Some(chosen)
    } else {
        parses(command.default_binding).then(|| command.default_binding.to_string())
    }
}

/// Applies `edit` to the command `id`: saves `keymap.toml`, then re-binds the
/// command - an `Unbind` of the key it had, and a binding for the key it now
/// has. Nothing is bound or changed in memory if saving fails. An unknown
/// `id` is a no-op.
pub fn apply(cx: &mut App, id: &str, edit: Edit) -> std::io::Result<()> {
    let mapper = cx.keyboard_mapper().clone();
    let registry = cx.global::<CommandRegistry>();
    let Some(command) = registry.get(id) else {
        return Ok(());
    };
    let live = cx.global::<LiveKeymap>();
    let mut config = live.config.clone();
    match edit {
        Edit::Set(keys) => {
            config
                .bindings
                .insert(id.to_string(), keys.trim().to_string());
        }
        Edit::Reset => {
            config.bindings.remove(id);
        }
        Edit::Remove => {
            config.bindings.insert(id.to_string(), String::new());
        }
    }
    crate::config::save(&live.path, &config)?;

    let old = bound_keys(command, &live.config, mapper.as_ref());
    let new = bound_keys(command, &config, mapper.as_ref());
    let mut changes = Vec::new();
    if old != new {
        if let Some(old) = &old {
            let unbind = Box::new(Unbind(command.action.name().into()));
            changes.extend(load_binding(command, old, unbind, mapper.as_ref()));
        }
        if let Some(new) = &new {
            changes.extend(load_binding(
                command,
                new,
                command.action.boxed_clone(),
                mapper.as_ref(),
            ));
            // Bound after the new key, the chords it starts stay reachable:
            // GPUI keeps a chord pending against a complete shorter binding
            // only when the chord was bound later (see `super::bindings`).
            for other in registry.iter().filter(|other| other.id != command.id) {
                if let Some(keys) = bound_keys(other, &config, mapper.as_ref())
                    && starts(new, &keys)
                {
                    changes.extend(load_binding(
                        other,
                        &keys,
                        other.action.boxed_clone(),
                        mapper.as_ref(),
                    ));
                }
            }
        }
    }
    cx.global_mut::<LiveKeymap>().config = config;
    cx.bind_keys(changes);
    Ok(())
}

/// Whether `prefix` is a proper prefix of `keys`, keystroke by keystroke
/// after parsing, so equivalent spellings compare equal.
fn starts(prefix: &str, keys: &str) -> bool {
    let parse = |keys: &str| -> Option<Vec<gpui_kit::Keystroke>> {
        keys.split_whitespace()
            .map(|key| gpui_kit::Keystroke::parse(key).ok())
            .collect()
    };
    match (parse(prefix), parse(keys)) {
        (Some(prefix), Some(keys)) => prefix.len() < keys.len() && keys.starts_with(&prefix),
        _ => false,
    }
}
