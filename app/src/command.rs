use gpui_kit::Action;

/// Where a command sits in the application menu: which of the seven top-level
/// menus and, in the menus that group their items, which group. A command
/// names at most one - the menu is a curated subset of commands, not every
/// command sorted into a bucket, so most commands (panel-scoped shortcuts
/// especially) carry `None` and stay palette/keymap-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuSlot {
    /// Between About and Services: Settings… (`settings.open`). About, Hide
    /// and Quit are platform items `ui::menu` builds itself.
    App,
    /// This app's stand-in for a conventional File menu: there are no
    /// documents to open/save/close, but there is a cluster context to pick
    /// and switch - see `app-menu-and-fonts/design.md` on why this is named
    /// `Context` rather than forcing a `File` label onto content that isn't
    /// files.
    Context(ContextGroup),
    // No `Edit` slot: no command sits in the Edit menu yet, which the menu bar
    // still draws (`TopMenu::Edit`) for the platform's text-editing items.
    View(ViewGroup),
    /// Global navigation only (`menu-organization`): commands that act
    /// whichever panel has focus. A panel-scoped command stays out of the menu
    /// bar, so the bar never depends on what has focus.
    Navigate(NavigateGroup),
    Window,
    Help,
}

/// The Context menu's groups, in menu order: context actions, then every
/// tunnel action together at the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextGroup {
    Contexts,
    Tunnels,
}

/// The View menu's groups, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViewGroup {
    /// The command palette, the way into every other command.
    Palette,
    /// Theme and text size.
    Appearance,
    /// Focusing, moving and collapsing the Resource panel, and its side
    /// preference.
    ResourcePanel,
    /// Maximizing the focused panel.
    PanelLayout,
    /// Table columns.
    TableColumns,
}

/// The Navigate menu's groups, in menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NavigateGroup {
    /// Moving focus between panels and to the Resource panel.
    Panels,
    /// Cycling tabs.
    Tabs,
    /// Selecting a tab by position: drawn as one "Select Tab" submenu, so nine
    /// near-identical items don't bury the rest.
    TabPositions,
}

/// The seven top-level menus, without the groups within them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopMenu {
    App,
    Context,
    Edit,
    View,
    Navigate,
    Window,
    Help,
}

impl MenuSlot {
    /// The top-level menu this slot is in.
    pub fn menu(self) -> TopMenu {
        match self {
            MenuSlot::App => TopMenu::App,
            MenuSlot::Context(_) => TopMenu::Context,
            MenuSlot::View(_) => TopMenu::View,
            MenuSlot::Navigate(_) => TopMenu::Navigate,
            MenuSlot::Window => TopMenu::Window,
            MenuSlot::Help => TopMenu::Help,
        }
    }

    /// The slot's group within its menu, by position; one group for the menus
    /// that don't group their items. Items sort by it, and a separator goes
    /// wherever it changes.
    pub fn group(self) -> u8 {
        match self {
            MenuSlot::Context(group) => group as u8,
            MenuSlot::View(group) => group as u8,
            MenuSlot::Navigate(group) => group as u8,
            MenuSlot::App | MenuSlot::Window | MenuSlot::Help => 0,
        }
    }

    /// The submenu this slot's items are drawn in, rather than inline.
    pub fn submenu(self) -> Option<&'static str> {
        match self {
            MenuSlot::Navigate(NavigateGroup::TabPositions) => Some("Select Tab"),
            MenuSlot::Navigate(NavigateGroup::Panels | NavigateGroup::Tabs)
            | MenuSlot::App
            | MenuSlot::Context(_)
            | MenuSlot::View(_)
            | MenuSlot::Window
            | MenuSlot::Help => None,
        }
    }
}

/// A registered command: metadata (for the palette and keymap) plus the
/// GPUI [`Action`] it dispatches. Wraps GPUI's action system rather than
/// replacing it - see design D4.
pub struct Command {
    /// Stable identity, used by `keymap.toml` and the registry lookup. Once
    /// chosen, never change it.
    pub id: &'static str,
    pub title: &'static str,
    pub default_binding: &'static str,
    /// `None` means globally available; `Some(name)` gates the command to
    /// windows/views with that `KeyContext` active.
    pub context: Option<&'static str>,
    pub action: Box<dyn Action>,
    /// Which top-level application menu shows this command, if any.
    pub menu: Option<MenuSlot>,
}

impl Command {
    /// Whether the command applies where `active_contexts` are on the focus
    /// path. A context is a context name, or names joined by `&&`, each
    /// possibly negated with `!` (`"ObjectListPanel && !Input"`), read as its
    /// key binding reads it - so a command bound outside text fields is offered
    /// outside them too.
    pub fn is_available(&self, active_contexts: &[&str]) -> bool {
        match self.context {
            None => true,
            Some(context) => context.split("&&").all(|term| {
                let term = term.trim();
                match term.strip_prefix('!') {
                    Some(name) => !active_contexts.contains(&name.trim()),
                    None => active_contexts.contains(&term),
                }
            }),
        }
    }
}

#[derive(Default)]
pub struct CommandRegistry {
    commands: Vec<Command>,
}

/// Stored as a GPUI global so any view (e.g. the palette trigger) can read
/// it without threading a reference through every layer.
impl gpui_kit::Global for CommandRegistry {}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, command: Command) {
        self.commands.push(command);
    }

    pub fn get(&self, id: &str) -> Option<&Command> {
        self.commands.iter().find(|command| command.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter()
    }

    /// Commands available given the active `KeyContext` stack.
    pub fn available<'a>(&'a self, active_contexts: &[&str]) -> Vec<&'a Command> {
        self.commands
            .iter()
            .filter(|command| command.is_available(active_contexts))
            .collect()
    }

    /// Commands in `menu`, grouped: by their slot's group in menu order, and in
    /// registration order within a group - the order the palette lists them,
    /// so the menu and the palette agree.
    pub fn for_menu(&self, menu: TopMenu) -> Vec<&Command> {
        let mut commands: Vec<&Command> = self
            .commands
            .iter()
            .filter(|command| command.menu.is_some_and(|slot| slot.menu() == menu))
            .collect();
        // Stable, so registration order holds within a group.
        commands.sort_by_key(|command| command.menu.map(MenuSlot::group));
        commands
    }

    /// Dispatches the command's action if it's registered and available in
    /// `active_contexts`; a context-gated command with its context inactive
    /// is inert. Returns whether it dispatched.
    // UNWIRED: the palette trigger dispatches through GPUI's own action
    // system directly today; no caller routes through this by id yet.
    #[allow(dead_code)]
    pub fn dispatch(&self, id: &str, active_contexts: &[&str], cx: &mut gpui_kit::App) -> bool {
        let Some(command) = self.get(id) else {
            return false;
        };
        if !command.is_available(active_contexts) {
            return false;
        }
        cx.dispatch_action(command.action.as_ref());
        true
    }
}

/// Case-insensitive subsequence match: every character of `query`, in
/// order, appears somewhere in `candidate` (not necessarily contiguous).
/// This is what makes "new win" match "New Window".
// UNWIRED: the palette (`util::palette`) still relies on gpui-component's own
// substring filtering; nothing calls this stronger match yet.
#[allow(dead_code)]
pub fn fuzzy_match(query: &str, candidate: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let candidate = candidate.to_lowercase();
    let mut candidate_chars = candidate.chars();
    query
        .to_lowercase()
        .chars()
        .all(|q| candidate_chars.any(|c| c == q))
}

/// Test-only: the palette itself (`util::palette`) builds its rows from
/// [`CommandRegistry::available`] directly, so it can dispatch where focus was; these
/// tests pin that same context gating.
///
/// Builds palette items for every command available in `active_contexts`,
/// using gpui-component's own `Command` palette - it already does
/// substring filtering and shows each item's active keybinding, so this
/// just supplies the entries. See [`fuzzy_match`] for the stronger
/// (subsequence) matching this module contributes on top.
#[cfg(test)]
pub fn build_items(
    registry: &CommandRegistry,
    active_contexts: &[&str],
) -> Vec<gpui_kit::component::command::CommandItem> {
    registry
        .available(active_contexts)
        .into_iter()
        .map(|command| {
            gpui_kit::component::command::CommandItem::new()
                .label(command.title)
                .action(command.action.boxed_clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Command, CommandRegistry, fuzzy_match};
    use gpui_kit::{TestAppContext, actions};
    use std::cell::Cell;
    use std::rc::Rc;

    actions!(command_test, [TestAction]);

    fn registry_with(context: Option<&'static str>) -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        registry.register(Command {
            id: "test.command",
            title: "Test Command",
            default_binding: "cmd-t",
            context,
            action: Box::new(TestAction),
            menu: None,
        });
        registry
    }

    #[test]
    fn registered_command_is_retrievable_by_id() {
        let registry = registry_with(None);
        assert!(registry.get("test.command").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[gpui_kit::test]
    fn dispatch_invokes_action_only_when_context_is_active(cx: &mut TestAppContext) {
        let count = Rc::new(Cell::new(0));
        let count_for_handler = count.clone();
        cx.update(|cx| {
            cx.on_action(move |_: &TestAction, _cx| {
                count_for_handler.set(count_for_handler.get() + 1);
            });
        });

        let registry = registry_with(Some("Editor"));

        let dispatched = cx.update(|cx| registry.dispatch("test.command", &[], cx));
        assert!(!dispatched, "inactive context should report no dispatch");
        assert_eq!(count.get(), 0, "inactive context should be inert");

        let dispatched = cx.update(|cx| registry.dispatch("test.command", &["Editor"], cx));
        assert!(dispatched);
        assert_eq!(count.get(), 1, "active context should invoke the action");
    }

    #[test]
    fn fuzzy_match_finds_subsequences_case_insensitively() {
        assert!(fuzzy_match("new win", "New Window"));
        assert!(fuzzy_match("nw", "New Window"));
        assert!(fuzzy_match("", "New Window"));
        assert!(!fuzzy_match("xyz", "New Window"));
    }

    #[test]
    fn palette_items_respect_context_gating() {
        let mut registry = CommandRegistry::new();
        registry.register(Command {
            id: "global.command",
            title: "Global Command",
            default_binding: "cmd-g",
            context: None,
            action: Box::new(TestAction),
            menu: None,
        });
        registry.register(Command {
            id: "scoped.command",
            title: "Scoped Command",
            default_binding: "cmd-s",
            context: Some("Editor"),
            action: Box::new(TestAction),
            menu: None,
        });

        assert_eq!(super::build_items(&registry, &[]).len(), 1);
        assert_eq!(super::build_items(&registry, &["Editor"]).len(), 2);
    }
}
