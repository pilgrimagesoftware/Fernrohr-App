//! The events browser's shortcuts as registry commands, scoped to the panel's
//! key context: each has a palette entry while an events browser has focus, a
//! `keymap.toml` override, and its key through `keymap::bindings`. What each
//! does lives with the panel's handlers.

use crate::command::{Command, CommandRegistry};
use gpui_kit::{Action, actions};

actions!(
    events_browser,
    [
        OpenInvolvedObject,
        FocusSearch,
        FilterByType,
        FilterByKind,
        FilterByReason,
        ClearFilters
    ]
);

/// The panel's key context.
pub const PANEL_KEY_CONTEXT: &str = "EventsPanel";
/// [`PANEL_KEY_CONTEXT`] outside the search box, which needs these letters typed.
pub(super) const KEY_CONTEXT: &str = "EventsPanel && !Input";

/// The default keys, also the hint row's fallback when the keymap has none.
pub(super) const OPEN_KEY: &str = "enter";
pub(super) const SEARCH_KEY: &str = "/";
pub(super) const TYPE_KEY: &str = "t";
pub(super) const KIND_KEY: &str = "k";
pub(super) const REASON_KEY: &str = "r";
pub(super) const CLEAR_KEY: &str = "x";

/// Registers the panel's shortcuts. None is in the menu bar, like every
/// panel-scoped command (`menu-organization`).
pub fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(KEY_CONTEXT),
            action,
            menu: None,
        });
    };
    register(
        "events.open_involved",
        "Events: Open Involved Object",
        OPEN_KEY,
        Box::new(OpenInvolvedObject),
    );
    register(
        "events.search",
        "Events: Search",
        SEARCH_KEY,
        Box::new(FocusSearch),
    );
    register(
        "events.filter_type",
        "Events: Filter by Type",
        TYPE_KEY,
        Box::new(FilterByType),
    );
    register(
        "events.filter_kind",
        "Events: Filter by Object Kind",
        KIND_KEY,
        Box::new(FilterByKind),
    );
    register(
        "events.filter_reason",
        "Events: Filter by Reason",
        REASON_KEY,
        Box::new(FilterByReason),
    );
    register(
        "events.clear_filters",
        "Events: Clear Filters",
        CLEAR_KEY,
        Box::new(ClearFilters),
    );
    registry.register(crate::ui::namespace_picker::pick_namespaces_command(
        "events.pick_namespaces",
        "Events: Pick Namespaces",
        PANEL_KEY_CONTEXT,
    ));
}
