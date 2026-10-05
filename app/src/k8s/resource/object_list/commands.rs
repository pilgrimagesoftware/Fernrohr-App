//! A list panel's shortcuts as registry commands: the actions, the keys they
//! default to, and their registration - so each has a palette entry while a list
//! panel is on the focus path, a `keymap.toml` override, and its key through
//! `keymap::bindings`. What each action does lives with the panel's handlers.

use crate::command::{Command, CommandRegistry};
use crate::k8s::resource::pod_detail::DetailView;
use crate::ui::nav::{ObjectTarget, OpenMode};
use gpui_kit::{Action, actions};

actions!(
    object_list,
    [
        FocusFilter,
        OpenSelected,
        DescribeSelected,
        ShowSelectedYaml,
        WarpNamespace,
        RefreshList,
        PortForwardService,
        OpenInBackground,
        EditSelected,
        DeleteSelected
    ]
);

/// Edits one object's YAML in `context_name`: opens (or focuses) its detail
/// panel on the YAML and starts the edit there, once loaded - what `e` on a
/// list row (any list, Pods too) and in a pod's detail panel dispatch, so every
/// route reuses the object panel's one editor and apply path. A Secret's
/// panel says why it can't be edited instead.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = object_list, no_json)]
pub struct EditListedObject {
    pub context_name: String,
    pub target: ObjectTarget,
}

/// Opens one listed object's detail panel in `context_name` - what Enter, `d`,
/// `y`, double-click and the row menu's "Open" dispatch. Carries its object, like
/// `ui::link::FollowReference`, so the window's one open path (which focuses an
/// already-open panel rather than adding a second) is all it needs. `view` is
/// the view to show, `None` for whichever the panel is on (structured, new);
/// `mode` whether it opens in the background (`open-in-background`).
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = object_list, no_json)]
pub struct OpenListedObject {
    pub context_name: String,
    pub target: ObjectTarget,
    pub view: Option<DetailView>,
    pub mode: OpenMode,
}

/// The panel's key context.
pub const PANEL_KEY_CONTEXT: &str = "ObjectListPanel";
/// [`PANEL_KEY_CONTEXT`] minus the filter box, where every command here is bound:
/// `w`, `/` and Enter type into the filter, so they mean the panel's actions only
/// outside it - the Resource panel's `LIST_KEY_CONTEXT` rule.
pub(crate) const LIST_KEY_CONTEXT: &str = "ObjectListPanel && !Input";

/// The default keys, also the hint bar's fallback when the keymap has none.
pub(super) const FILTER_KEY: &str = "/";
pub(super) const OPEN_KEY: &str = "enter";
/// Open in Background: the platform modifier with Enter (`cmd-enter` on macOS).
pub(super) const OPEN_IN_BACKGROUND_KEY: &str = "secondary-enter";
pub(super) const NAMESPACE_KEY: &str = "w";
/// The Pods table's describe and YAML keys (`pods::DESCRIBE_KEY`, `YAML_KEY`), so
/// every list answers them alike (`standard-resource-panels` 5.1).
pub(super) const DESCRIBE_KEY: &str = "d";
pub(super) const YAML_KEY: &str = "y";
/// Re-lists a polled kind now (`unwatchable-kinds`).
pub(super) const REFRESH_KEY: &str = "r";
/// Around a list's table while its kind can be patched: Edit's context.
pub const EDITABLE_KEY_CONTEXT: &str = "EditableList";
/// k9s's edit key, as the object panel's (`object_detail::EDIT_KEY`).
pub(crate) const EDIT_KEY: &str = "e";

const FILTER_COMMAND_ID: &str = "object_list.focus_filter";
const OPEN_COMMAND_ID: &str = "object_list.open";
const OPEN_IN_BACKGROUND_COMMAND_ID: &str = "object_list.open_in_background";
const NAMESPACE_COMMAND_ID: &str = "object_list.warp_namespace";
const DESCRIBE_COMMAND_ID: &str = "object_list.describe";
const YAML_COMMAND_ID: &str = "object_list.yaml";
const FIT_COMMAND_ID: &str = "object_list.fit_columns";
const REFRESH_COMMAND_ID: &str = "object_list.refresh";
/// The Pods list's delete key, so every list deletes alike.
pub(super) const DELETE_KEY: &str = "ctrl-d";
/// k9s's port-forward key, as the Pods panel's.
pub(super) const PORT_FORWARD_KEY: &str = "shift-f";

/// Registers the panel's shortcuts. None is in the menu bar, like their Pods
/// twins: the bar holds global commands only (`menu-organization`), and the
/// palette offers these while a list panel is on the focus path.
pub fn register_commands(registry: &mut CommandRegistry) {
    registry.register(Command {
        id: "services.port_forward",
        title: "Services: Port-Forward Selected Service",
        default_binding: PORT_FORWARD_KEY,
        context: Some("ServicesList && !Input"),
        action: Box::new(PortForwardService),
        menu: None,
    });
    // Only in a list of a kind discovery says can be patched.
    registry.register(Command {
        id: "object_list.edit",
        title: "List: Edit Selected Object's YAML",
        default_binding: EDIT_KEY,
        context: Some("EditableList && !Input"),
        action: Box::new(EditSelected),
        menu: None,
    });
    // Only in a list of a kind discovery says can be deleted (`delete`).
    registry.register(Command {
        id: "object_list.delete",
        title: "List: Delete Selected Object",
        default_binding: DELETE_KEY,
        context: Some("DeletableList && !Input"),
        action: Box::new(DeleteSelected),
        menu: None,
    });
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
        None,
    );
    register(
        OPEN_IN_BACKGROUND_COMMAND_ID,
        "List: Open Selected Object in Background",
        OPEN_IN_BACKGROUND_KEY,
        Box::new(OpenInBackground),
        None,
    );
    register(
        DESCRIBE_COMMAND_ID,
        "List: Describe Selected Object",
        DESCRIBE_KEY,
        Box::new(DescribeSelected),
        None,
    );
    register(
        YAML_COMMAND_ID,
        "List: Show Selected Object's YAML",
        YAML_KEY,
        Box::new(ShowSelectedYaml),
        None,
    );
    register(
        NAMESPACE_COMMAND_ID,
        "List: Filter to Selected Object's Namespace",
        NAMESPACE_KEY,
        Box::new(WarpNamespace),
        None,
    );
    // The keyboard twin of a double-click on a header divider (`ui::table_fit`).
    register(
        FIT_COMMAND_ID,
        "List: Fit Columns to Contents",
        crate::ui::table_fit::FIT_COLUMNS_KEY,
        Box::new(crate::ui::table_fit::FitAllColumns),
        None,
    );
    // Re-lists a polled kind now; a watched kind is already current, so it
    // does nothing there.
    register(
        REFRESH_COMMAND_ID,
        "List: Refresh",
        REFRESH_KEY,
        Box::new(RefreshList),
        None,
    );
    registry.register(crate::ui::namespace_picker::pick_namespaces_command(
        "object_list.pick_namespaces",
        "List: Pick Namespaces",
        LIST_KEY_CONTEXT,
    ));
}
