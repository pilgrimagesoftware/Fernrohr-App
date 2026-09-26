use gpui_kit::Action;

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
}

impl Command {
    pub fn is_available(&self, active_contexts: &[&str]) -> bool {
        match self.context {
            None => true,
            Some(context) => active_contexts.contains(&context),
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

    // UNWIRED: no caller looks a command up by id outside `dispatch` (also
    // unwired) and this module's own tests yet.
    #[allow(dead_code)]
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
// UNWIRED: `build_items` below still relies on gpui-component's own
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

/// Builds palette items for every command available in `active_contexts`,
/// using gpui-component's own `Command` palette - it already does
/// substring filtering and shows each item's active keybinding, so this
/// just supplies the entries. See [`fuzzy_match`] for the stronger
/// (subsequence) matching this module contributes on top.
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
        });
        registry.register(Command {
            id: "scoped.command",
            title: "Scoped Command",
            default_binding: "cmd-s",
            context: Some("Editor"),
            action: Box::new(TestAction),
        });

        assert_eq!(super::build_items(&registry, &[]).len(), 1);
        assert_eq!(super::build_items(&registry, &["Editor"]).len(), 2);
    }
}
