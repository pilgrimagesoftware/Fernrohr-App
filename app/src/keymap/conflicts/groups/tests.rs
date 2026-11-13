//! The groups against the registry and the default keymap (#331, #391).

use super::{STANDALONE, groups};
use crate::command::CommandRegistry;
use crate::keymap::{KeymapConfig, conflicts};
use gpui_kit::KeyBindingContextPredicate;

fn registry() -> CommandRegistry {
    let mut registry = CommandRegistry::new();
    crate::util::shell::register_commands(&mut registry);
    registry
}

/// The identifiers `predicate` requires.
fn required(predicate: &KeyBindingContextPredicate, into: &mut Vec<String>) {
    match predicate {
        KeyBindingContextPredicate::Identifier(name) => into.push(name.to_string()),
        KeyBindingContextPredicate::And(left, right) => {
            required(left, into);
            required(right, into);
        }
        _ => {}
    }
}

/// #391: a builder that gains a flag can't silently leave `conflicts` blind
/// to it - every identifier a command requires is in a group or declared
/// standalone.
#[test]
fn every_identifier_is_grouped_or_standalone() {
    let mut missing = Vec::new();
    for command in registry().iter() {
        let Some(context) = command.context else {
            continue;
        };
        let predicate = KeyBindingContextPredicate::parse(context).expect("parses");
        let mut names = Vec::new();
        required(&predicate, &mut names);
        for name in names {
            let grouped = groups().iter().any(|group| group.contains(name.as_str()));
            if !grouped && !STANDALONE.contains(&name.as_str()) && !missing.contains(&name) {
                missing.push(name);
            }
        }
    }
    missing.sort();
    assert!(
        missing.is_empty(),
        "neither grouped nor standalone: {missing:?}"
    );
}

/// The end-to-end guard: no default key clashes with another in a scope that
/// can be live at the same time, so the editor never warns about a default.
#[test]
fn no_default_binding_clashes() {
    let registry = registry();
    let config = KeymapConfig::default();
    let clashes: Vec<String> = registry
        .iter()
        .filter(|command| !command.default_binding.is_empty())
        .filter_map(|command| {
            let found = conflicts(&registry, &config, command.id, command.default_binding);
            found.any_clash().then(|| {
                format!(
                    "{} ({}): {:?} {:?}",
                    command.id, command.default_binding, found.same_scope, found.prefixes
                )
            })
        })
        .collect();
    assert!(clashes.is_empty(), "{clashes:#?}");
}
