//! Which keys would complete a half-typed chord (`pending-chord-indicator`):
//! given the keystrokes GPUI is holding and the focused context stack, every
//! command whose effective key starts with them and is live there.
//!
//! The keys are each command's effective binding - its `keymap.toml` override,
//! else its default - built the way [`super::bindings`] builds them, so a
//! rebound chord lists its new key. A command is live where its context
//! predicate matches some depth of the stack, the test GPUI's own dispatch
//! applies, so `!Input` keeps a chord's completions out of a text field.

use super::{KeymapConfig, load_binding, resolve};
use crate::command::CommandRegistry;
use gpui_kit::{KeyContext, Keystroke, PlatformKeyboardMapper};

/// One way to finish the pending chord.
// UNWIRED(#131): read by the status bar's chord indicator, next section.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// The keys still to press, after the pending ones.
    pub remaining: Vec<Keystroke>,
    pub id: &'static str,
    pub title: &'static str,
}

/// Every command whose effective key extends `pending` and is live in
/// `contexts` (lowest to highest, as `Window::context_stack` gives it), in
/// registration order - the command palette's order. Empty for no pending
/// keys, or keys that start no command's chord.
// UNWIRED(#131): called by the status bar's chord indicator, next section.
#[allow(dead_code)]
pub fn completions(
    registry: &CommandRegistry,
    config: &KeymapConfig,
    pending: &[Keystroke],
    contexts: &[KeyContext],
    mapper: &dyn PlatformKeyboardMapper,
) -> Vec<Completion> {
    if pending.is_empty() {
        return Vec::new();
    }
    registry
        .iter()
        .filter_map(|command| {
            let chosen = resolve(command.id, command.default_binding, config);
            if chosen.is_empty() {
                return None;
            }
            let action = || command.action.boxed_clone();
            let binding = load_binding(command, &chosen, action(), mapper)
                .or_else(|| load_binding(command, command.default_binding, action(), mapper))?;
            let keys = binding.keystrokes();
            let extends = keys.len() > pending.len()
                && pending
                    .iter()
                    .zip(keys)
                    .all(|(typed, bound)| typed.should_match(bound));
            let live = binding
                .predicate()
                .is_none_or(|predicate| predicate.depth_of(contexts).is_some());
            (extends && live).then(|| Completion {
                remaining: keys[pending.len()..]
                    .iter()
                    .map(|key| key.inner().clone())
                    .collect(),
                id: command.id,
                title: command.title,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
