//! Which other commands a key would collide with, for the keybindings
//! editor's warning.
//!
//! Every registered command is either global or gated to a key context. Two
//! commands conflict when both are global or their contexts can match at
//! once (`scope`, #331). A global and a panel command on one key don't
//! conflict - GPUI gives the deeper (panel) binding precedence inside that
//! panel and the global one everywhere else - but the editor shows it.
//!
//! A key that starts another command's chord (`cmd-k` against `cmd-k right`)
//! is a clash too, in either direction: one of the two stops working. That
//! one is reported wherever the scopes overlap - both global, one global, or
//! the same context - since precedence doesn't untangle a half-typed chord.

mod groups;
mod scope;

use super::{KeymapConfig, resolve};
use crate::command::CommandRegistry;
use gpui_kit::Keystroke;

/// A panel command taking a key inside its panel that a global command has
/// everywhere else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shadow {
    pub panel_command: &'static str,
    pub global_command: &'static str,
    pub context: &'static str,
}

/// How the new key and another command's key overlap as chords.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefixKind {
    /// The new key is the start of the other command's longer chord.
    StartsTheirs,
    /// The other command's key is the start of the new, longer chord.
    StartsMine,
}

/// Another command whose key and the new key are one a prefix of the other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefix {
    pub command: &'static str,
    pub kind: PrefixKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conflicts {
    /// Commands that would share the key in the same scope.
    pub same_scope: Vec<&'static str>,
    /// Global/panel pairs on the key, resolved by precedence.
    pub shadows: Vec<Shadow>,
    /// Commands in an overlapping scope whose chord the key starts, or whose
    /// key starts the key's chord.
    pub prefixes: Vec<Prefix>,
}

impl Conflicts {
    /// Whether the key clashes with anything the editor should ask about.
    pub fn any_clash(&self) -> bool {
        !self.same_scope.is_empty() || !self.prefixes.is_empty()
    }
}

/// What giving command `id` the key `keys` would collide with, given every
/// other command's current key in `config`. Keys compare after parsing, so
/// equivalent spellings (`shift-cmd-p`, `cmd-shift-p`) match; `keys` that
/// don't parse collide with nothing.
pub fn conflicts(
    registry: &CommandRegistry,
    config: &KeymapConfig,
    id: &str,
    keys: &str,
) -> Conflicts {
    let mut found = Conflicts::default();
    let (Some(target), Some(command)) = (parse_keys(keys), registry.get(id)) else {
        return found;
    };
    for other in registry.iter().filter(|other| other.id != id) {
        let other_keys = resolve(other.id, other.default_binding, config);
        let Some(other_keys) = parse_keys(&other_keys) else {
            continue;
        };
        if other_keys != target {
            if let Some(kind) = prefix_kind(&target, &other_keys)
                && scope::overlap(command.context, other.context)
            {
                found.prefixes.push(Prefix {
                    command: other.id,
                    kind,
                });
            }
            continue;
        }
        match (command.context, other.context) {
            (None, None) => found.same_scope.push(other.id),
            // The same panel collides, as does a narrower clause on it
            // (`Logs` against `Logs && !Input`, #331); two different panels
            // are never both the focus path's deepest context, so they don't.
            (Some(mine), Some(theirs)) => {
                if scope::contexts_overlap(mine, theirs) {
                    found.same_scope.push(other.id);
                }
            }
            (None, Some(context)) => found.shadows.push(Shadow {
                panel_command: other.id,
                global_command: command.id,
                context,
            }),
            (Some(context), None) => found.shadows.push(Shadow {
                panel_command: command.id,
                global_command: other.id,
                context,
            }),
        }
    }
    found
}

/// Whether one of `mine` and `theirs` is a proper prefix of the other.
fn prefix_kind(mine: &[Keystroke], theirs: &[Keystroke]) -> Option<PrefixKind> {
    if mine.len() < theirs.len() && theirs.starts_with(mine) {
        Some(PrefixKind::StartsTheirs)
    } else if theirs.len() < mine.len() && mine.starts_with(theirs) {
        Some(PrefixKind::StartsMine)
    } else {
        None
    }
}

/// Whether `keys` starts with a keystroke that has no modifier other than
/// shift - a plain letter, which as a *global* key would fire while typing in
/// a field that doesn't handle it.
pub fn lacks_modifier(keys: &str) -> bool {
    parse_keys(keys)
        .and_then(|keys| keys.into_iter().next())
        .is_some_and(|first| {
            let modifiers = first.modifiers;
            !(modifiers.control || modifiers.alt || modifiers.platform || modifiers.function)
        })
}

fn parse_keys(keys: &str) -> Option<Vec<Keystroke>> {
    let parsed: Option<Vec<Keystroke>> = keys
        .split_whitespace()
        .map(|key| Keystroke::parse(key).ok())
        .collect();
    parsed.filter(|keys| !keys.is_empty())
}
