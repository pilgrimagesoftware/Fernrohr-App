//! The Keyboard Shortcuts list as data: one row per registered command, what
//! it shows, and what the filter matches. Kept apart from the view so the
//! rules (scope names, the changed marker, shadow notes) test without a window.

use crate::command::CommandRegistry;
use crate::keymap::{self, KeymapConfig};

/// One command's row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: &'static str,
    pub title: &'static str,
    /// "Global", or the panel the command is gated to ("Pods panel").
    pub scope: String,
    /// The key it has now; `None` for none.
    pub keys: Option<String>,
    /// The key differs from the command's default.
    pub changed: bool,
    /// Where another command shares this key across scopes, in words.
    pub notes: Vec<String>,
}

/// Every registered command's row, in registration order (the palette's
/// order), from `config`'s current overrides.
pub fn rows(registry: &CommandRegistry, config: &KeymapConfig) -> Vec<Row> {
    registry
        .iter()
        .map(|command| {
            let keys = keymap::resolve(command.id, command.default_binding, config);
            let keys = (!keys.is_empty()).then_some(keys);
            let changed = keys.as_deref().unwrap_or("") != command.default_binding;
            let mut notes = Vec::new();
            if let Some(keys) = &keys {
                for shadow in keymap::conflicts(registry, config, command.id, keys).shadows {
                    let other = if shadow.panel_command == command.id {
                        shadow.global_command
                    } else {
                        shadow.panel_command
                    };
                    let other_title = registry.get(other).map_or(other, |c| c.title);
                    let panel = scope_name(Some(shadow.context));
                    notes.push(if shadow.panel_command == command.id {
                        format!("In the {panel}, this takes the key from “{other_title}”.")
                    } else {
                        format!("In the {panel}, “{other_title}” takes this key.")
                    });
                }
                if command.context.is_none() && keymap::lacks_modifier(keys) {
                    notes.push(
                        "A plain key on a global command fires while typing in fields that \
                         don't use it."
                            .to_string(),
                    );
                }
            }
            Row {
                id: command.id,
                title: command.title,
                scope: scope_name(command.context),
                keys,
                changed,
                notes,
            }
        })
        .collect()
}

/// Whether `row` matches the filter `query`: case-insensitively, in its
/// title, id or key.
pub fn matches(row: &Row, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || row.title.to_lowercase().contains(&query)
        || row.id.to_lowercase().contains(&query)
        || row
            .keys
            .as_deref()
            .is_some_and(|keys| keys.to_lowercase().contains(&query))
}

/// A key context's name for people: `None` is "Global"; `PodDetailPanel`
/// reads "Pod Detail panel"; any other name is split into words.
pub fn scope_name(context: Option<&str>) -> String {
    let Some(context) = context else {
        return "Global".to_string();
    };
    let mut words: Vec<String> = Vec::new();
    for ch in context.chars() {
        if ch.is_uppercase() || words.is_empty() {
            words.push(ch.to_string());
        } else if let Some(last) = words.last_mut() {
            last.push(ch);
        }
    }
    if words.last().is_some_and(|last| last == "Panel") {
        words.pop();
        format!("{} panel", words.join(" "))
    } else {
        words.join(" ")
    }
}

#[cfg(test)]
mod tests;
