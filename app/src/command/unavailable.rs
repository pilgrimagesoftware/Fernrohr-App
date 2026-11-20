//! Why a registered command can't run where focus is (#406), so the palette can
//! list it disabled with the reason rather than run it and report why after.
//!
//! A command's key context says *where* it applies; whether it can run *now*
//! depends on the state of the panel holding focus (Helm's Rollback needs a
//! usable `helm` for that panel's context, and an earlier revision). So a panel
//! registers a reason function against its own focus handle, and the palette
//! asks the ones whose element contains the focused one. This module doesn't
//! stop a key binding from running the command: the panel's handler still
//! refuses, as before.

use gpui_kit::{Action, App, FocusHandle, Global, SharedString, WeakFocusHandle, Window};

/// What a panel says about one action: `Some(reason)` when it can't run now.
type ReasonFn = Box<dyn Fn(&dyn Action, &App) -> Option<SharedString>>;

struct Source {
    focus: WeakFocusHandle,
    reason: ReasonFn,
}

/// Every live panel's reason function, keyed by the panel's focus handle.
#[derive(Default)]
struct UnavailableReasons {
    sources: Vec<Source>,
}

impl Global for UnavailableReasons {}

/// Registers `reason` for commands run while focus is inside `focus`. Held
/// weakly: a source whose panel is gone (its last focus handle dropped) is
/// skipped, and pruned on the next registration.
pub(crate) fn register(
    focus: &FocusHandle,
    reason: impl Fn(&dyn Action, &App) -> Option<SharedString> + 'static,
    cx: &mut App,
) {
    let reasons = cx.default_global::<UnavailableReasons>();
    reasons
        .sources
        .retain(|source| source.focus.upgrade().is_some());
    reasons.sources.push(Source {
        focus: focus.downgrade(),
        reason: Box::new(reason),
    });
}

/// Why `action` can't run where focus is in `window`, if a panel on the focus
/// path says it can't. Read before anything (the palette) takes focus away.
pub(crate) fn reason(action: &dyn Action, window: &Window, cx: &App) -> Option<SharedString> {
    let reasons = cx.try_global::<UnavailableReasons>()?;
    reasons
        .sources
        .iter()
        .filter(|source| {
            source
                .focus
                .upgrade()
                .is_some_and(|focus| focus.contains_focused(window, cx))
        })
        .find_map(|source| (source.reason)(action, cx))
}

#[cfg(test)]
mod tests;
