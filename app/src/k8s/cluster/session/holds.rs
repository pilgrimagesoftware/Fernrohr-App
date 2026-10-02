//! Per-window holds on a context's session: `hold`, `release`, `release_window`, and
//! the holder count they maintain - keyed by window rather than by panel.

use super::*;

impl ClusterRegistry {
    /// Adds `window_id` to `context_name`'s holder set, connecting (or reusing) its
    /// session first if none exists yet. Design decision 1: a window's hold on a
    /// context is independent of whether any panel has subscribed to a watch - the
    /// Resource panel can list kinds for a context with no open panels.
    pub fn hold(cx: &mut App, context_name: &str, window_id: WindowId) {
        Self::ensure_init(cx, context_name);
        cx.global_mut::<Self>()
            .sessions
            .get_mut(context_name)
            .expect("ensure_init just inserted this session")
            .holders
            .insert(window_id);
    }

    /// Removes `window_id` from `context_name`'s holder set. On the last release,
    /// drops the session entirely - its connection (and so, via `ClusterConnection`'s
    /// own `Drop`, its tunnel forward) and its watches. A no-op if the context has no
    /// session, or `window_id` wasn't holding it.
    ///
    /// `window-context-bar` section 3.3's Disconnect calls this directly for the one
    /// context being disconnected; closing a window releases every context it holds
    /// at once, through [`Self::release_window`] instead.
    pub fn release(cx: &mut App, context_name: &str, window_id: WindowId) {
        if !cx.has_global::<Self>() {
            return;
        }
        let should_remove = {
            let Some(session) = cx.global_mut::<Self>().sessions.get_mut(context_name) else {
                return;
            };
            session.holders.remove(&window_id);
            session.holders.is_empty()
        };
        if should_remove {
            cx.global_mut::<Self>().sessions.remove(context_name);
        }
    }

    /// Releases every hold `window_id` has, across every context - what closing a
    /// window does (design.md decision 2's "closing a window releases all its
    /// holds"), without the caller needing to know which contexts that window used.
    pub fn release_window(cx: &mut App, window_id: WindowId) {
        if !cx.has_global::<Self>() {
            return;
        }
        let emptied: Vec<String> = cx
            .global_mut::<Self>()
            .sessions
            .iter_mut()
            .filter_map(|(context_name, session)| {
                session.holders.remove(&window_id);
                session.holders.is_empty().then(|| context_name.clone())
            })
            .collect();
        for context_name in emptied {
            cx.global_mut::<Self>().sessions.remove(&context_name);
        }
    }

    /// How many windows currently hold `context_name` - `0` for a context with no
    /// session (never held, or its last holder already released it). The disconnect
    /// confirmation (`window-context-bar` section 3.3) reads this minus one (itself)
    /// for "stays connected in N other windows".
    pub fn holder_count(cx: &App, context_name: &str) -> usize {
        cx.try_global::<Self>()
            .and_then(|registry| registry.sessions.get(context_name))
            .map(|session| session.holders.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
