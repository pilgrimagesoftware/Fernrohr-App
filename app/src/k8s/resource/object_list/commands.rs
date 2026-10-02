//! A list panel's shortcuts as registry commands: the actions, the keys they
//! default to, and their registration - so each has a palette entry while a list
//! panel is on the focus path, a `keymap.toml` override, and its key through
//! `keymap::bindings`. What each action does lives with the panel's handlers.

use crate::command::{Command, CommandRegistry, MenuSlot};
use crate::ui::nav::ObjectTarget;
use gpui_kit::{Action, actions};

actions!(object_list, [FocusFilter, OpenSelected, WarpNamespace]);

/// Opens one listed object's detail panel in `context_name` - what Enter,
/// double-click and the row menu's "Open" dispatch. Carries its object, like
/// `ui::link::FollowReference`, so the window's one open path (which focuses an
/// already-open panel rather than adding a second) is all it needs.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = object_list, no_json)]
pub struct OpenListedObject {
    pub context_name: String,
    pub target: ObjectTarget,
}

/// The panel's key context.
pub const PANEL_KEY_CONTEXT: &str = "ObjectListPanel";
/// [`PANEL_KEY_CONTEXT`] minus the filter box, where every command here is bound:
/// `w`, `/` and Enter type into the filter, so they mean the panel's actions only
/// outside it - the Resource panel's `LIST_KEY_CONTEXT` rule.
pub(super) const LIST_KEY_CONTEXT: &str = "ObjectListPanel && !Input";

/// The default keys, also the hint bar's fallback when the keymap has none.
pub(super) const FILTER_KEY: &str = "/";
pub(super) const OPEN_KEY: &str = "enter";
pub(super) const NAMESPACE_KEY: &str = "w";

const FILTER_COMMAND_ID: &str = "object_list.focus_filter";
const OPEN_COMMAND_ID: &str = "object_list.open";
const NAMESPACE_COMMAND_ID: &str = "object_list.warp_namespace";

/// Registers the panel's shortcuts. Open sits in Navigate, the namespace warp in
/// View (both like their Pods twins); focusing the filter is palette-only.
pub fn register_commands(registry: &mut CommandRegistry) {
    let mut register = |id, title, default_binding, action: Box<dyn Action>, menu| {
        registry.register(Command {
            id,
            title,
            default_binding,
            context: Some(LIST_KEY_CONTEXT),
            action,
            menu,
        });
    };
    register(
        FILTER_COMMAND_ID,
        "List: Focus Filter",
        FILTER_KEY,
        Box::new(FocusFilter),
        None,
    );
    register(
        OPEN_COMMAND_ID,
        "List: Open Selected Object",
        OPEN_KEY,
        Box::new(OpenSelected),
        Some(MenuSlot::Navigate),
    );
    register(
        NAMESPACE_COMMAND_ID,
        "List: Filter to Selected Object's Namespace",
        NAMESPACE_KEY,
        Box::new(WarpNamespace),
        Some(MenuSlot::View),
    );
}
