//! `window-context-bar`: the pure decision logic behind a window's context list -
//! kept free of GPUI so `util/shell.rs` (already near its file-size cap) only has to
//! wire these results into `WindowMode::Workspace`'s fields, the dock, and
//! `ClusterRegistry`, rather than working the decisions out inline.
//!
//! Every function here is a plain value in, value out computation, which is what
//! makes each independently unit-testable without a `TestAppContext`.

/// A separator no kubeconfig context name can contain - ASCII Unit Separator,
/// a control character that never appears in a context's YAML string. Used by
/// [`dock_layout_key`] to join more than one context name into one map key.
const CONTEXT_KEY_SEPARATOR: char = '\u{1f}';

/// The key a window's dock arrangement is filed under in `SavedDockLayouts`
/// (`config/dock_layouts.rs`) - task 2.2's save side of workspace persistence.
///
/// A single-context window keeps the historical bare context-name key, so an
/// existing `dock-layouts.json` written before this change still applies. A
/// multi-context window's arrangement is filed under *every* context it uses,
/// sorted order-insensitively and joined with [`CONTEXT_KEY_SEPARATOR`] - so two
/// windows on the same *set* of contexts, added in either order, share one
/// arrangement, and adding or dropping a context (which changes the set) starts
/// a fresh arrangement rather than silently reusing a stale one.
///
/// `contexts` must not be empty - a `Picker`-mode window has nothing to key.
pub(crate) fn dock_layout_key(contexts: &[String]) -> String {
    let mut sorted: Vec<&str> = contexts.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.join(&CONTEXT_KEY_SEPARATOR.to_string())
}

/// Where a window's `contexts` and `active` index land after Disconnect removes
/// `context_name` (`window-context-bar` section 3.3). `None` means removing it
/// leaves no context at all - the window falls back to the cluster picker instead
/// of holding an `active` index into an empty list.
///
/// `active` shifts down by one when the removed context sat at or before it, so it
/// keeps naming the same *context* (or, if that context was the one removed, the
/// nearest one before it) rather than drifting to whatever now happens to occupy
/// its old numeric slot.
pub(crate) fn contexts_after_disconnect(
    contexts: &[String],
    active: usize,
    context_name: &str,
) -> Option<(Vec<String>, usize)> {
    let Some(index) = contexts.iter().position(|c| c == context_name) else {
        // Not a context this window uses - nothing changes.
        return Some((contexts.to_vec(), active));
    };
    let mut remaining = contexts.to_vec();
    remaining.remove(index);
    if remaining.is_empty() {
        return None;
    }
    let shifted = if index <= active {
        active.saturating_sub(1)
    } else {
        active
    };
    let clamped = shifted.min(remaining.len() - 1);
    Some((remaining, clamped))
}

/// The Disconnect confirmation's body text (`window-context-bar` section 3.3): how
/// many of this window's panels will close, and, when `other_windows` is nonzero,
/// that `context_name` stays connected elsewhere.
pub(crate) fn disconnect_confirmation_body(
    context_name: &str,
    panel_count: usize,
    other_windows: usize,
) -> String {
    let panels = match panel_count {
        0 => "No open panels".to_string(),
        1 => "1 open panel".to_string(),
        n => format!("{n} open panels"),
    };
    let mut body = format!("{panels} for {context_name} will close.");
    if other_windows > 0 {
        let windows = match other_windows {
            1 => "1 other window".to_string(),
            n => format!("{n} other windows"),
        };
        body.push(' ');
        body.push_str(&format!("{context_name} stays connected in {windows}."));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::{contexts_after_disconnect, disconnect_confirmation_body, dock_layout_key};

    #[test]
    fn a_single_context_keys_by_its_bare_name() {
        assert_eq!(dock_layout_key(&["kind-dev".to_string()]), "kind-dev");
    }

    #[test]
    fn multiple_contexts_key_sorted_and_joined_regardless_of_input_order() {
        let a = dock_layout_key(&["staging".to_string(), "kind-dev".to_string()]);
        let b = dock_layout_key(&["kind-dev".to_string(), "staging".to_string()]);
        assert_eq!(a, b, "the key must not depend on add order");
        assert_eq!(a, "kind-dev\u{1f}staging");
    }

    #[test]
    fn disconnecting_an_unused_context_changes_nothing() {
        let contexts = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            contexts_after_disconnect(&contexts, 1, "never-used"),
            Some((contexts, 1))
        );
    }

    #[test]
    fn disconnecting_the_only_context_returns_none() {
        assert_eq!(contexts_after_disconnect(&["a".to_string()], 0, "a"), None);
    }

    #[test]
    fn disconnecting_a_context_before_active_shifts_active_down() {
        let contexts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            contexts_after_disconnect(&contexts, 2, "a"),
            Some((vec!["b".to_string(), "c".to_string()], 1))
        );
    }

    #[test]
    fn disconnecting_a_context_after_active_leaves_active_alone() {
        let contexts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            contexts_after_disconnect(&contexts, 0, "c"),
            Some((vec!["a".to_string(), "b".to_string()], 0))
        );
    }

    #[test]
    fn disconnecting_the_active_context_clamps_to_the_new_last_index() {
        let contexts = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            contexts_after_disconnect(&contexts, 1, "b"),
            Some((vec!["a".to_string()], 0))
        );
    }

    #[test]
    fn confirmation_names_the_panel_count_with_no_other_window() {
        assert_eq!(
            disconnect_confirmation_body("carefulcrab", 3, 0),
            "3 open panels for carefulcrab will close."
        );
        assert_eq!(
            disconnect_confirmation_body("carefulcrab", 1, 0),
            "1 open panel for carefulcrab will close."
        );
        assert_eq!(
            disconnect_confirmation_body("carefulcrab", 0, 0),
            "No open panels for carefulcrab will close."
        );
    }

    #[test]
    fn confirmation_names_a_single_other_window() {
        assert_eq!(
            disconnect_confirmation_body("greedygoat", 2, 1),
            "2 open panels for greedygoat will close. \
             greedygoat stays connected in 1 other window."
        );
    }

    #[test]
    fn confirmation_names_several_other_windows() {
        assert_eq!(
            disconnect_confirmation_body("greedygoat", 2, 3),
            "2 open panels for greedygoat will close. \
             greedygoat stays connected in 3 other windows."
        );
    }
}
